---
name: version-bump
description: 同步更新三处版本号（package.json、Cargo.toml、tauri.conf.json），用户说"升级版本号"时触发
---

# 版本号同步更新

用户说"升级版本号"时，按以下步骤执行：

## 步骤

1. **确认新版本号**：询问用户目标版本号，或根据当前版本自动递增（patch +1）
2. **读取当前版本**：检查 package.json 中的 version 字段
3. **同步更新三处**：
   - `package.json` → `"version": "X.Y.Z"`
   - `src-tauri/Cargo.toml` → `version = "X.Y.Z"`
   - `src-tauri/tauri.conf.json` → `"version": "X.Y.Z"`
4. **验证一致性**：读取三处确认版本号一致

## 文件位置

| 文件 | 字段 |
|------|------|
| `package.json` | `"version"` |
| `src-tauri/Cargo.toml` | `version` |
| `src-tauri/tauri.conf.json` | `"version"` |

## 注意事项

- 三处版本号必须完全一致
- 不要自动提交，等用户指示
- 版本号格式：`MAJOR.MINOR.PATCH`
