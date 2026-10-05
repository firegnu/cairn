# agent hooks 资料摘录

2026-10-05 查阅。Claude Code 和 Codex 两节，是用 `curl` 下载官方页面的 Markdown 原文后在本地核对的，没有依赖网页抓取工具的摘要。以下都是**文档写明的能力，还没在本机验证**；本机是否支持，以阶段 1 实测为准（[实施计划](../实施计划.md)）。

## 1. Claude Code

来源：https://code.claude.com/docs/en/hooks （Markdown 版：`hooks.md`）

### 通用

- 所有匹配到的 hooks **并行运行**。同一个 handler 如果写在多个 settings 文件里，只运行一次；插件或技能里的同名副本则各自运行。
- 写 hooks 的位置：
  - `~/.claude/settings.json`（用户级）
  - `.claude/settings.json`（项目级，可以提交进仓库）
  - `.claude/settings.local.json`（项目本地，不提交）
  - 托管策略
  - 插件的 `hooks/hooks.json`
  - 技能或子 agent 的 frontmatter
- 直接修改 settings 文件里的 hooks，一般会被文件监听自动加载。
- **工作区信任**：交互式会话里，所有 settings 文件的 hooks（包括 `~/.claude/settings.json`）都要等用户接受该目录的信任对话框后才会运行。`-p`、SDK 会话不弹对话框，直接把目录当作已信任。
- **退出码**：
  - 0：stdout 如果是以 `{` 开头、`}` 结尾的 JSON 就按 JSON 解析，否则当纯文本。SessionStart、UserPromptSubmit 的纯文本会注入上下文。
  - 2：表示阻断；对 Stop 来说是续跑。
  - 其他值且没有有效 JSON：属于非阻断错误，会话里会出现 `<hook name> hook error` 提示，stderr 进调试日志。
- `additionalContext`、`systemMessage` 等字符串上限 10,000 字符，超出的部分落盘，只给路径加前 2,000 字符的预览。
- **默认超时**：command hook 600 秒；UserPromptSubmit 30 秒；SessionEnd 共享 1.5 秒预算（单个 hook 设了更长的 `timeout` 会把预算抬高，最多 60 秒；也可以用环境变量 `CLAUDE_CODE_SESSIONEND_HOOKS_TIMEOUT_MS`）。

### 公共输入字段（节选）

- `session_id`
- `prompt_id`：当前这条用户提示的 UUID。第一次用户输入之前没有这个字段。要求 v2.1.196 及以上。
- `transcript_path`：异步写入，可能跟不上内存里的对话；**cairn 不读**。
- `cwd`
- `permission_mode`

### SessionStart

- 匹配器的取值：`startup`、`resume`（`--resume`、`--continue`、`/resume`）、`clear`、`compact`、`fork`。
- 交互式启动、启动时恢复会话、执行 `/clear` 时，SessionStart 在后台运行，但 Claude 的第一条回复会等它跑完。
- 输入：`source`，以及可选的 `model`、`agent_type`、`session_title`。`resume` 和 `fork` 还会带上"距离上次回复的秒数"等字段。
- 输出：纯文本 stdout，或者 `hookSpecificOutput.additionalContext`，注入在第一条提示之前。

### UserPromptSubmit

- 不只是用户打字时触发。定时任务（含 `/loop`）、后台子 agent 回报、其他会话发来的消息也会触发它。
- 输入里有 `prompt`（**cairn 不读不存**）和公共字段。

### Stop

- 原文："Runs when the main Claude Code agent has finished responding. Does not run if the stoppage occurred due to a user interrupt. API errors fire StopFailure instead."
- 输入：`stop_hook_active`（因 stop hook 而续跑时为 true）、`last_assistant_message`、`background_tasks`、`session_crons`。
- 续跑：输出 `{"decision":"block","reason":"…"}` 或用退出码 2。`additionalContext` 也能让对话继续，标记为 `Stop hook feedback`。
- **连续续跑上限 8 次**；Claude 每调用一次工具，计数就重置。可以用 `CLAUDE_CODE_STOP_HOOK_BLOCK_CAP` 调整。

### SessionEnd

- `reason` 的取值：`clear`、`resume`、`logout`、`prompt_input_exit`、`other`。
- 不能阻止会话结束，JSON 输出会被丢弃。

## 2. Codex

来源：https://learn.chatgpt.com/docs/hooks （Markdown 版：`hooks.md`）

### 通用

- 匹配到的所有 hooks 都会运行；同一事件的多个 command hook **并发启动**。
- 写 hooks 的位置：`~/.codex/hooks.json`、`~/.codex/config.toml` 的 `[hooks]` 表、`<repo>/.codex/hooks.json`、`<repo>/.codex/config.toml`，以及插件自带的 hooks。多个来源会全部加载，高优先级的层不会覆盖低优先级层里的 hooks。
- 项目级 hooks 只有在项目的 `.codex/` 层受信任时才加载。
- **信任**：非托管的 hook 运行前，必须由用户审核并信任这一份确切的定义。信任记在 hook 当前的哈希上，**新增或修改过的 hook 会被标为待审核，在被信任之前一直跳过**。在 CLI 里用 `/hooks` 管理。`--dangerously-bypass-hook-trust` 只对这一次调用免信任。
- 命令在会话的 `cwd` 下执行。
- 超时：默认 600 秒；SessionEnd 和 Interrupt 默认 1 秒，最多 3 秒。
- `additionalContextLimit`（写在 handler 上）：默认约 2,500 token，超出的部分落盘，只给预览。设为 0 表示不限制，但不建议。
- 关闭 hooks：`[features] hooks = false`。

### 公共输入与输出

- 输入：`session_id`、`transcript_path`（格式不稳定，**cairn 不读**）、`cwd`、`hook_event_name`、`model`。与回合相关的事件另外带 `turn_id`。
- 输出：`continue`、`stopReason`、`systemMessage`、`suppressOutput`。退出码 0 且没有输出，表示成功、继续执行。

### SessionStart

- `source` 的取值：`startup`、`resume`、`clear`、`compact`。
- 纯文本 stdout 会作为额外的开发者上下文注入，也可以用 `hookSpecificOutput.additionalContext`。
- compact 之后，SessionStart 会在下一次模型请求之前运行；压缩如果发生在回合中途，就在紧接着的续跑之前运行。

### UserPromptSubmit

- 输入：`turn_id`、`prompt`（**cairn 不读不存**）。

### Stop

- 输入：`turn_id`、`stop_hook_active`（"这一回合是否已经被 Stop 续跑过"）、`last_assistant_message`（"如果有"）。
- 退出码为 0 时，stdout 必须是 JSON，纯文本无效。
- 续跑：输出 `{"decision":"block","reason":"…"}`。原文："tells Codex to continue and automatically creates a new continuation prompt that acts as a new user prompt, using your reason as that prompt text." 所以续跑后的 `turn_id` 是否改变，**要在阶段 1 实测**。
- 只要有一个匹配的 Stop hook 返回 `continue: false`，就优先于其他 hook 的续跑决定。

### SessionEnd

- 以下情况会触发：正常关闭；归档或删除仍打开着的会话；会话闲置 30 分钟且没有任何客户端打开它。
- `reason` 目前总是 `other`。总是同步执行；它的输出不会影响 Codex。

### Interrupt

- 用户中断主线程上正在进行的回合时触发，带上被中断回合的 `turn_id`。不能阻止中断，也不能重启回合。

## 3. 恢复原会话（参考）

- Claude Code：`claude --continue` 恢复当前目录下最近的一个会话；`claude --resume` 打开选择列表。https://code.claude.com/docs/en/common-workflows#resume-previous-conversations
- Codex：有 `codex resume`。按 ID 恢复的确切写法没有核实。https://learn.chatgpt.com/docs/codex/cli

## 4. Claude Code 自动记忆（对照）

来源：https://code.claude.com/docs/en/memory

- 由 Claude 自己写，分为 user、feedback、project、reference 四类。每个会话加载 `MEMORY.md` 的前 200 行或前 25KB。
- 保存在 `~/.claude/projects/<project>/memory/`，按仓库划分，各 worktree 共享，只在本机。
- CLAUDE.md 和自动记忆都只是上下文，不是强制配置。
- 和 cairn 的关系：自动记忆偏向长期偏好和学习，而且只有 Claude 能用；cairn 做的是跨工具的工作接续。**cairn 不读这些文件。**

## 5. Corral 注入 hooks 的方式（与 cairn 共存相关）

来源：Saddle 仓库 `crates/corral-core/src/hooks.rs`，只读参考。

- 启动 Claude 时，加上 `--settings '{"hooks":{…}}'`。注入的事件有 SessionStart、UserPromptSubmit、PreToolUse、PostToolUse、PermissionRequest、Notification、Stop、StopFailure、SessionEnd，命令是 `<helper> __hook <事件>`，超时 10 秒。
- 启动 Codex 时，对每个事件加上 `-c hooks.<事件>=[…]`，并加上 `--dangerously-bypass-hook-trust`。
- helper 是 Corral 状态目录下 `.runtime/helper` 这个固定软链接，用来在升级后保持 hook 定义不变。cairn 的稳定命令路径借用了同样的思路。
- Corral 自己的 hook 会把部分字段写进它的事件文件。**cairn 不读这个文件。**

## 6. 社区工具（参考）

- beads：https://github.com/gastownhall/beads 。一个面向 agent 的任务图加记忆。用 `bd prime` 注入上下文，用 `bd setup claude|codex` 安装 hooks，存储用 Dolt。
- engram：https://github.com/Gentleman-Programming/engram 。Go 写的单个二进制加 SQLite（含 FTS5），由 agent 主动调用 `mem_save` 等 MCP 工具保存，按项目划分。
- Anthropic 的工程文章：https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents 。用进度文件加 Git 日志，新会话先读。
