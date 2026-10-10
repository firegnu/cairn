# cairn

cairn 是给终端里 coding agent 用的工作接续记忆工具：新会话开局自动看到上次停在哪、现场变化和下一步建议；每个正常回合，agent 都有一次保存机会。用 Rust 写，独立运行，不依赖 Saddle、Corral、Drover。

设计以 `docs/DESIGN.md` 为准。实现中要改设计，先改那份文档，并在提交说明里写清楚。

## 先读

- `docs/DESIGN.md`：权威设计。§2 已定决定，§14 风险，§15 待定问题。
- `docs/实施计划.md`：分几个阶段，以及实施门槛。**阶段 1 的能力实测没通过之前，不派核心实现任务。**
- `docs/背景与决策记录.md`：用户原话、为什么这样定、哪些方案已被否决（不要重新争论）。
- `docs/调研/agent-hooks资料.md`：Claude Code 和 Codex 官方 hooks 资料摘录，以及 Corral 的 hooks 注入方式。
- `HANDOFF.md`：现在在哪、下一步做什么。

## 规矩

- **语言与依赖**：Rust stable；只用成熟、活跃维护的库。实现、测试和辅助脚本都不用 Python（沿用用户在 Saddle/Corral 的要求）。Git 信息通过调用 `git` 命令取得，调用要设超时。
- **独立**：不依赖 Saddle、Corral、Drover 的代码，也不读它们的数据或内部状态，比如 Corral 的事件文件。和 Corral 注入的 hooks 只做共存实测。需要参考 Saddle 仓库时只读，不修改。
- **隐私**：开发和测试时，以下内容一律不读：
  - 本机真实的会话记录（`~/.claude/projects/**` 下的 transcript、`~/.codex` 下的会话记录等）；
  - 各工具自带的记忆文件；
  - 用户真实的 cairn 数据库。

  hook 代码不保存 `prompt` 和 `last_assistant_message`，也不读 `transcript_path`。测试只用合成材料、临时 Git 仓库和隔离目录（临时 `HOME` / `XDG_STATE_HOME`）。
- **不改用户真实配置**：开发期间不写 `~/.claude/settings.json`、`~/.codex/hooks.json`、`~/.codex/config.toml`。实测时只用会话级参数注入（Claude 用 `--settings`，Codex 用 `-c hooks.*`），或者临时目录里的项目级配置。真实的 `cairn install` 必须等用户明确同意后才执行。需要用户点信任对话框时，先告诉用户，并说明之后怎么清除。
- **不要干扰用户正在用的 agent**：`corral ls` 里现有的 agent 都是用户的。可以用 `corral ls/status/reply` 读它们；不要对它们执行 `corral stop`、`corral send`、`corral keys`，也不要 attach 上去打字。主控按分派流程开出来的 `cairn/dev-*`、`cairn/test-*` 是自己的，照流程送话、关闭。需要真实 agent 做实测时，自己开一个 `cairn/test-<名字>`，用完立刻 `corral stop`。
- **不按名字批量杀进程**：不要用 `pkill -f cairn`、`pkill -f corral` 这类命令，主控和其他 agent 的命令行里都带着项目名和工作目录。停自己起的进程，用启动时记下的 PID。
- **测试不依赖真实 agent**：hook 适配用合成的 JSON 夹具测试。阶段 1 的实测是唯一例外，因为它就是要验证真实 agent 的能力。
- **验证**：按改动影响面选择检查。小改动跑直接相关的测试；跨模块改动、阶段集成和合并前，跑 `cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings`。保留有价值的测试，不靠删测试、放宽断言或缩短超时来通过检查。
- **共用编译目录**：所有 worktree 共用一个编译目录，命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 开发方式（主控分派）

- 这个项目的开发任务由主控拆开，派给别的 agent 做。主控负责拆任务、写任务文件、审查、合并，不自己写功能代码。分派时按 corral-dispatch 技能做。
- 被委派的 agent（任务文件里写明了身份）照任务文件做，不再往下派。
- 需求单只写用户要的结果；验收照抄用户原话，不补验收点。主控觉得该加的，列出来问用户。`docs/实施计划.md` 里的"建议验证"是设计方的检查思路，不是用户的验收条件。
- agent 名字以 `cairn/dev-` 开头；任务文件放 `docs/tasks/`；每个任务一个分支，worktree 放 `../cairn-worktrees/<分支>`，交叉审查用 detached worktree `../cairn-worktrees/review-<分支>`。
- 创建派发的 agent 时，在 `corral start` 参数里注明职责：实现者加 `--label role=implementer`，独立审查者加 `--label role=reviewer`，实测用的 agent 加 `--label role=test`。标签只用于显示，不改变职责分工或权限。
- 审查：主控审查每个任务。
- 合并：审查通过后，本地合并进 main，再推送到 origin（`github.com/firegnu/cairn`，公开仓库）。主控不自行创建远程仓库。
- 收尾记号：一件活合并完、worktree 和分支都清理干净之后，在 main 上补一条空提交（`git commit --allow-empty`），首行写「收尾: 」加一句话说明这件活是什么。只记真正落地的活；说好不合并、停在审查的不记。
- 收尾之后更新 `HANDOFF.md`：现在在哪、下一步做什么、有什么悬而未决。设计和理由写进 `docs/DESIGN.md`，不写进交接文件。
- 开出来的 agent：清理某个 worktree 时，把住在里面的那个 agent 一并关掉（它的工作目录没了，接不了新活）；其余的，用户说关才关。
