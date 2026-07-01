import { existsSync, unlinkSync } from "fs"
import { join } from "path"

export default {
  "tool.execute.before": async (input: any, output: any) => {
    if (input.tool === "bash") {
      const cmd: string = input.args?.command || ""

      // 只拦截 git add 和 git commit
      if (!/\bgit\s+(add|commit)\b/.test(cmd)) return

      // 检查许可标记文件
      const flagPath = join(input.args?.workdir || process.cwd(), ".git-commit-allowed")
      if (existsSync(flagPath)) {
        // 有许可，放行并删除标记
        unlinkSync(flagPath)
        return
      }

      // 无许可，拦截
      output.cancel = true
      output.cancelReason = "⚠️ git add/commit 被拦截。需要用户明确说"提交"后才能执行。"
    }
  },
}
