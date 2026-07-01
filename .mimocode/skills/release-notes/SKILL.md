---
name: release-notes
description: 从 git log 提取版本公告，格式化为 GitHub Release 发布内容，用户说"整理版本公告"时触发
---

# 版本公告生成

用户说"整理版本公告"时，按以下步骤执行：

## 步骤

1. **获取当前版本号**：从 `package.json` 读取当前版本号
2. **查找上一个版本提交**：`git log --oneline` 找到格式如 `升级版本号至X.Y.Z` 的提交
3. **提取变更**：`git log --oneline <上一版本commit>..HEAD` 获取变更列表
4. **分类整理**：按提交前缀分类
5. **格式化输出**：用模板输出

## Git 提交分类规则

- `fix:` → 修复
- `feat:` → 新增
- `refactor:` → 改进
- `perf:` → 改进（性能）
- `docs:` → 文档
- `chore:` → 忽略（版本号、依赖等）

## 输出模板

```
## MeowMic vX.X.X 更新公告

---

**🐱🎤 MeowMic vX.X.X 已发布！**

<一句话概述本次更新重点>

### 🔧 修复
- <修复项>

### ✨ 改进
- <改进项>

### 📜 文档
- <文档变更>（如有）

---

📥 **下载**：[GitHub Releases](https://github.com/NanCheng-L/MeowMic/releases/latest)
💬 **反馈**：[GitHub Issues](https://github.com/NanCheng-L/MeowMic/issues)
```

## 注意事项

- 版本号前加 `v`（如 `0.2.14` → `v0.2.14`）
- 只输出最新版本的内容
- 如果某个分类为空，不输出该分类标题
- 输出内容直接显示给用户，方便复制到 GitHub Release
