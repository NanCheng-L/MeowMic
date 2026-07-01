---
name: git-commit
description: 检查工作区状态并提交代码，用户说"提交代码"或"提交git"时触发
---

# Git 提交工作流

用户说"提交代码"或"提交git"时，按以下步骤执行：

## 步骤

1. **创建许可标记**：在项目根目录创建 `.git-commit-allowed` 文件（git-guard hook 需要）
2. **检查状态**：运行 `git status` 查看变更文件
3. **查看差异**：运行 `git diff --stat` 了解变更规模
4. **查看最近提交**：运行 `git log --oneline -3` 了解提交风格
5. **分析变更**：判断是修复、新增、改进还是重构
6. **暂存文件**：`git add` 相关文件（不要 add .env、密钥、token 等敏感文件）
7. **提交**：用中文写 commit message，格式：`<类型>: <描述>`
   - 类型：fix / feat / refactor / docs
   - 示例：`fix: 系统托盘退出选项缺失`
8. **验证**：运行 `git status` 确认提交成功

## 注意事项

- 不要提交 node_modules、target、.env 等文件
- commit message 用中文
- 如果有未跟踪的文件需要确认是否加入
- 不要主动 push，等用户指示
- 必须先创建 `.git-commit-allowed` 再执行 git add/commit，否则 hook 会拦截
