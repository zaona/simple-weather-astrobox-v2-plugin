# 项目初始化说明

## 沟通语言
中文

## 开发文档与参考
- 根目录开发文档：`README.md`
- 参考插件源码：`Daymatter-AstroBox-Plugin/`
- 官方插件文档：`AstroBox-NG-Plugin-Docs/`

## 常用命令（快速开始）
```bash
# 初始化子模块
git submodule update --init --remote --recursive

# 构建插件
python scripts/build_dist.py

# 打包插件
python scripts/build_dist.py --release --package
```

## 产物位置
- WASM：`dist/`
- ABP：`dist/`
