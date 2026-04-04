#!/usr/bin/env python3
import argparse
import json
import shutil
import subprocess
import sys
import zipfile
import os
from datetime import datetime, timezone
from pathlib import Path


def load_local_env(root_dir):
    env_path = root_dir / ".env.local"
    if not env_path.exists():
        return {}

    values = {}
    for raw_line in env_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if not key:
            continue

        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]

        values[key] = value

    return values


def run_cargo_build(root_dir, cargo_args, env):
    cmd = ["cargo", "build", *cargo_args]
    result = subprocess.run(cmd, cwd=root_dir, env=env)
    if result.returncode != 0:
        sys.exit(result.returncode)


def load_cargo_metadata(root_dir):
    cmd = ["cargo", "metadata", "--no-deps", "--format-version", "1"]
    result = subprocess.run(
        cmd,
        cwd=root_dir,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr or result.stdout)
        sys.exit(result.returncode)
    return json.loads(result.stdout)


def pick_root_package(metadata, root_manifest):
    packages = metadata.get("packages", [])
    for pkg in packages:
        manifest_path = Path(pkg.get("manifest_path", "")).resolve()
        if manifest_path == root_manifest:
            return pkg

    members = metadata.get("workspace_members", [])
    if members:
        member_id = members[0]
        for pkg in packages:
            if pkg.get("id") == member_id:
                return pkg

    return packages[0] if packages else None


def find_wasm(target_dir, profile, target, entry_name, crate_name):
    patterns = []
    if entry_name:
        patterns.append(entry_name)
    if crate_name:
        patterns.append(f"{crate_name}.wasm")
        patterns.append(f"lib{crate_name}.wasm")

    search_root = target_dir / target if target else target_dir
    if not search_root.exists():
        search_root = target_dir

    for pattern in patterns:
        matches = []
        for path in search_root.rglob(pattern):
            if not path.is_file():
                continue
            if profile and profile not in path.parts:
                continue
            matches.append(path)
        if matches:
            matches.sort(key=lambda p: p.stat().st_mtime, reverse=True)
            return matches[0]

    return None


def copy_item(root_dir, dist_dir, rel_path):
    if not rel_path:
        return

    src = Path(rel_path)
    if not src.is_absolute():
        src = root_dir / rel_path

    if not src.exists():
        sys.stderr.write(f"warning: path not found: {rel_path}\n")
        return

    dest_rel = rel_path.lstrip("/\\")
    dest = dist_dir / dest_rel

    if src.is_dir():
        dest.mkdir(parents=True, exist_ok=True)
        shutil.copytree(src, dest, dirs_exist_ok=True)
    else:
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dest)


def make_package_name(raw_name, fallback):
    name = (raw_name or "").strip()
    if not name:
        name = (fallback or "plugin").strip()
    if not name:
        name = "plugin"

    invalid = '<>:"/\\|?*'
    cleaned = []
    for ch in name:
        cleaned.append("_" if ch in invalid else ch)

    safe = "".join(cleaned).strip(" .")
    return safe or "plugin"


def package_dist(dist_dir, output_path):
    output_path.parent.mkdir(parents=True, exist_ok=True)
    files = [
        path
        for path in dist_dir.rglob("*")
        if path.is_file() and path.resolve() != output_path.resolve()
    ]

    with zipfile.ZipFile(output_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in files:
            archive.write(path, path.relative_to(dist_dir))


def sync_dist_to_release(root_dir, dist_dir):
    release_dir = root_dir / "release"
    release_dir.mkdir(parents=True, exist_ok=True)

    for child in release_dir.iterdir():
        if child.is_dir():
            shutil.rmtree(child)
        else:
            child.unlink()

    for item in dist_dir.iterdir():
        if item.is_file() and item.suffix.lower() == ".abp":
            continue
        dest = release_dir / item.name
        if item.is_dir():
            shutil.copytree(item, dest)
        else:
            shutil.copy2(item, dest)

def get_git_output(root_dir, args):
    result = subprocess.run(
        ["git", *args],
        cwd=root_dir,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return ""
    return (result.stdout or "").strip()

def collect_build_info(root_dir):
    build_time = datetime.now(timezone.utc).isoformat(timespec="seconds")
    git_user = get_git_output(root_dir, ["config", "user.name"])
    git_email = get_git_output(root_dir, ["config", "user.email"])
    git_hash = get_git_output(root_dir, ["rev-parse", "HEAD"])
    git_branch = get_git_output(root_dir, ["rev-parse", "--abbrev-ref", "HEAD"])

    if not git_user:
        git_user = os.environ.get("USER") or os.environ.get("LOGNAME") or "unknown"
    if not git_hash:
        git_hash = "unknown"
    if not git_branch:
        git_branch = "unknown"

    return {
        "AB_BUILD_TIME": build_time,
        "AB_BUILD_USER": git_user,
        "AB_BUILD_GIT_HASH": git_hash,
        "AB_BUILD_GIT_BRANCH": git_branch,
    }


def main():
    parser = argparse.ArgumentParser(
        description="Build wasm with cargo and package dist assets."
    )
    parser.add_argument("--release", action="store_true", help="Use release profile")
    parser.add_argument("--profile", help="Use a specific cargo profile")
    parser.add_argument("--target", help="Override cargo target triple")
    parser.add_argument(
        "--package",
        action="store_true",
        help="Package dist into <name>.abp",
    )
    args, extra = parser.parse_known_args()

    cargo_args = []
    profile = "debug"

    if args.profile:
        cargo_args.extend(["--profile", args.profile])
        profile = args.profile
    elif args.release:
        cargo_args.append("--release")
        profile = "release"

    if args.target:
        cargo_args.extend(["--target", args.target])

    cargo_args.extend(extra)

    root_dir = Path(__file__).resolve().parent.parent
    manifest_path = root_dir / "manifest.json"

    if not manifest_path.exists():
        sys.stderr.write(f"manifest.json not found: {manifest_path}\n")
        sys.exit(1)

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    plugin_name = manifest.get("name")
    entry = str(manifest.get("entry") or "")
    icon = str(manifest.get("icon") or "")
    additional = manifest.get("additional_files") or []

    env = os.environ.copy()
    is_strict_release = args.release and not args.package
    required_release_env_keys = [
        "WEATHER_API_HOST",
        "SUPABASE_URL",
        "SUPABASE_PUBLISHABLE_KEY",
    ]
    env_local_path = root_dir / ".env.local"
    if not env_local_path.exists():
        if is_strict_release:
            sys.stderr.write(
                "warning: 项目根目录未找到 .env.local。\n"
                "warning: 当前执行的是 `python scripts/build_dist.py --release`，"
                "该命令会让打包结果进入生产环境，但缺少必须配置，已阻止本次 release 构建。\n"
                "warning: 如果只是日常开发和调试，请使用 "
                "`python scripts/build_dist.py --release --package`。\n"
                "warning: 请参考 .env.example 创建 .env.local，配置 "
                "WEATHER_API_HOST、SUPABASE_URL、SUPABASE_PUBLISHABLE_KEY"
                "（该文件不会随 Git 提交）。\n"
            )
            sys.exit(1)

        sys.stderr.write(
            "warning: 项目根目录未找到 .env.local。"
            "该插件使用 Supabase 上报，请参考 .env.example 创建 .env.local，"
            "并在其中配置 WEATHER_API_HOST、SUPABASE_URL、SUPABASE_PUBLISHABLE_KEY"
            "（该文件不会随 Git 提交）。"
            "当前将继续构建。\n"
        )

    local_env = load_local_env(root_dir)
    if is_strict_release:
        missing = []
        for key in required_release_env_keys:
            local_val = local_env.get(key, "")
            env_val = env.get(key, "")
            value = local_val if str(local_val).strip() else env_val
            if not str(value).strip():
                missing.append(key)
        if missing:
            sys.stderr.write(
                "warning: 缺少 release 必填环境变量: "
                + ", ".join(missing)
                + "\n"
                + "warning: 请在 .env.local 中补齐后重试。\n"
            )
            sys.exit(1)

    for key, value in local_env.items():
        env.setdefault(key, value)
    env.update(collect_build_info(root_dir))
    run_cargo_build(root_dir, cargo_args, env)

    metadata = load_cargo_metadata(root_dir)
    target_dir = Path(metadata.get("target_directory", root_dir / "target"))

    root_manifest = (root_dir / "Cargo.toml").resolve()
    root_package = pick_root_package(metadata, root_manifest)
    crate_name = None
    if root_package:
        crate_name = root_package.get("name")
        if crate_name:
            crate_name = crate_name.replace("-", "_")

    entry_name = Path(entry).name if entry else ""
    wasm_path = find_wasm(target_dir, profile, args.target, entry_name, crate_name)
    if not wasm_path:
        sys.stderr.write(
            "wasm artifact not found. Looked for "
            f"{entry_name or crate_name or '*.wasm'} in {target_dir}\n"
        )
        sys.exit(1)

    dist_dir = root_dir / "dist"
    dist_dir.mkdir(parents=True, exist_ok=True)

    copy_item(root_dir, dist_dir, "manifest.json")

    wasm_dest_name = entry or wasm_path.name
    wasm_dest = dist_dir / wasm_dest_name.lstrip("/\\")
    wasm_dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(wasm_path, wasm_dest)

    copy_item(root_dir, dist_dir, icon)

    if isinstance(additional, list):
        for item in additional:
            if item is None:
                continue
            copy_item(root_dir, dist_dir, str(item))

    if args.package:
        package_name = make_package_name(plugin_name, crate_name)
        package_path = dist_dir / f"{package_name}.abp"
        package_dist(dist_dir, package_path)

    if args.release and not args.package:
        sync_dist_to_release(root_dir, dist_dir)


if __name__ == "__main__":
    main()
