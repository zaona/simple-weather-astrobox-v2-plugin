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

### 更新子模块

```bash
# Windows
update_submodules.bat

# Linux/macOS
./update_submodules.sh
```

### 构建插件

```bash
python scripts/build_dist.py
```

构建完成后，生成的 WASM 文件位于 `dist` 目录。

### 打包插件

```bash
python scripts/build_dist.py --release --package
```

构建完成后，生成的 ABP 文件位于 `dist` 目录。

### 安装到 AstroBox

将生成的 `daymatter_astrobox_v2_plugin.wasm` 文件和 `manifest.json` 放置到 AstroBox 插件目录中。