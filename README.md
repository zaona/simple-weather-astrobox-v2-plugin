# 简明天气 快应用 AstroBoxV2插件

> 🧩 simple-weather-astrobox-plugin-v2

---

## 项目简介

简明天气是适用于Vela的长期天气存储快应用

## 感谢
- [倒数日AstroBox插件](https://github.com/sf-yuzifu/Daymatter-AstroBox-Plugin) 项目
- [WaiJade](https://github.com/CheongSzesuen)

## 快应用包名
com.application.zaona.weather

## 快速开始

### 初始化子模块
```
git submodule update --init --remote --recursive
```

### 安装 Rust WASM target

项目默认通过 `.cargo/config.toml` 构建到 `wasm32-wasip2`，首次构建前先安装该 target：

```bash
rustup target add wasm32-wasip2
```

如果你的 Rust 不是通过 `rustup` 管理，需要先切到 `rustup` 工具链，或自行安装 `wasm32-wasip2` 对应标准库。

### 更新子模块

```bash
# Windows
update_submodules.bat

# Linux/macOS
./update_submodules.sh
```

### 构建插件

> release 强制要求使用本地配置文件（不要提交到仓库）：
>
> ```bash
> cp .env.example .env.local
> # 然后编辑 .env.local 填入真实值：
> # WEATHER_API_HOST、WEATHER_API_CLIENT_TYPE、WEATHER_API_KEY
> ```

### 开发命令

```bash
python scripts/build_dist.py --release --package
```

构建完成后，生成的 ABP 文件位于 `dist` 目录。

### 发版命令

```bash
python scripts/build_dist.py --release
```

运行后自动把内容复制到release文件夹，进入生产环境。
