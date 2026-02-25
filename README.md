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

### 发版命令

```bash
python scripts/build_dist.py --release
```

有利于用户加载插件，运行完毕后将文件替换到release里。

### 开发命令

```bash
python scripts/build_dist.py --dev
```

我在开发的时候输入上面两个命令了114514遍，所以有了这个的开发命令。一个单命令快捷参数 --dev，相当于“构建 + 打包”，默认走 release。
