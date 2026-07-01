---
name: update-json
description: 生成 latest.json 文件用于自动更新，用户说"更新json文件"时触发
---

# 生成 latest.json

用户说"更新json文件"时，按以下步骤执行：

## 打包默认目录

```
src-tauri\target\release\bundle\nsis\
```

## 步骤

1. **获取当前版本号**：从 `package.json` 读取当前版本号
2. **查找签名文件**：在打包目录中查找 `MeowMic_{version}_x64-setup.exe.sig`
3. **读取签名**：读取签名文件内容
4. **生成版本公告**：从 git log 提取或用版本公告模板生成
5. **运行生成脚本**：`node scripts/generate-update-json.cjs <版本号> <安装包路径>`
6. **验证**：确认 latest.json 已生成在安装包同目录

## 注意事项

- 版本号前加 `v`（如 `0.2.14` → `v0.2.14`）
- pub_date 使用当前时间，ISO 8601 格式
- 生成后提示用户手动上传到 GitHub Release
- 不要自动创建 Release 或上传文件
