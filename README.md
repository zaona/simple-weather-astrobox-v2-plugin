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

## 功能

- 把手机端（安卓 `ImageSyncManager`）的天气数据同步到手环快应用
- 端上直接选图导入（PNG / JPG / WEBP），压暗与模糊可调，出图统一按 RGB_565 量化
- 已配置的背景图按安卓 `ImageSyncManager` 那套分片协议推给手环快应用（覆盖式）
- `.swbg` 预设包导入导出，与安卓端互通

## 自定义背景图

传输协议与安卓端逐字对齐（JSON + base64 分片，单片 3072 字节）：

```text
手机 → 手环：{"type":"header","totalSize":N,"chunkSize":3072,"totalChunks":N,
             "width":N,"height":N,"weatherCode":"21","current":3,"total":12,"label":"晴-白天"}
          {"type":"data","index":N,"chunk":"<base64>"}
          {"type":"end"} / {"type":"clear_all"} / {"type":"cancel"}
手环 → 手机：{"type":"image_saved","weatherCode":"21"} / {"type":"clear_done"} / {"type":"cancel"}
```

推送流程（对齐安卓 `ImageSyncManager.syncAllImages`）：先确认手环端装了快应用并注册
接收，`advancedSyncMode` 开启时再拉起快应用握手（最多 5 次 `start`，等 `ready`），
然后按码表顺序逐张发送。每张图都从原件重新走一遍「按 inSampleSize 采样 → 等比缩放到
432×514 → 模糊 → 压暗 → RGB_565 量化 → PNG」，发完 `end` 后等手环回 `image_saved`，
超时 30 秒；手环中途回 `cancel`（手环上按了「取消传输」）立即中止整轮。

背景图页顶部那张「背景图传输」卡片就是安卓端发送按钮的位置：空闲时是「发送到手表」，
点它把当前已配置的图覆盖式推一遍（端上选图 / 导入预设包 / 改完压暗模糊之后都要点
一次才会同步到手环）；传输中变成「取消传输」，手环上点取消也会立刻中止整轮。
一张图都没配置时点发送会先二次确认，确认后清除手环上已存的自定义背景图（本机库不动）。

12 个天气编号：晴-白天/夜晚/日落（21/22/23）、多云-白天/阴-夜晚（11/12）、
阴-白天（31）、雾霾-白天/夜晚（41/42）、雨-白天/夜晚（51/52）、雪-白天/夜晚（61/62）。

文件布局：

```text
bg/source-{code}.{ext}     原件，端上选的；早期版本从手机端收来的图也在这里回退读取
bg/custom-bg-{code}.png    成品图，按当前压暗/模糊参数算出
bg/images.json             元信息（尺寸、文件名、原件扩展名）
api_settings.json          插件设置，含压暗与模糊
```

压暗、模糊是全局参数，滑块永远从原件重算，所以参数往回调不会把图越调越糊。

### .swbg 预设包

`.swbg` 就是 ZIP，字段名与安卓 `BackgroundPresetManager` 的 `@SerializedName`
一一对应，两端导出的包互相可导。导出默认文件名同样是 `weather_backgrounds.swbg`：

```text
manifest.json
  formatVersion / appVersion / exportTimestamp / metadata
  globalSettings: { darkenStrength, blurRadius, quality, advancedSyncMode }
  presets[]: { weatherCode, weatherLabel, imageFile, imageFormat,
               originalFileName, settings: { darkenStrength, blurRadius, quality } }
images/{code}.{ext}       原始图片，不是成品图（条目名以包内实际命名为准）
```

包里存的是原件加参数，所以来回导不会掉画质。导入时按码表顺序读 `images/`
下的条目，编号取自条目文件名；`globalSettings` 会覆盖插件本地的压暗 / 模糊。
`quality` 与 `advancedSyncMode` 插件端不参与处理（画质固定 RGB_565），但会
原样存下来，再次导出时带回，保证和安卓来回导不丢设置。

解码支持的格式与安卓 `BitmapFactory` 的常用子集一致：PNG / JPEG / WebP
（动图取第一帧）。打包里若有解码不了的原件，会在导入结果里计入「跳过」。

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
