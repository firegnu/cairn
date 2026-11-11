# 任务：cairn 接入 canter，作第三家 agent

2026-11-11，cairn/main 交给 cairn/dev-canter（Codex，碰要害：gpt-6-astra / xhigh）。
路由：碰要害 / 交叉审查要 / 影响面：碰要害（路由：重（置信 0.8）、要（数据模型 0.99）、碰要害）
交叉审查：用户 11-11 定换成 Codex（「审查换成 Codex」），不用读表默认的 Claude。
类型：功能变更
依据：本轮实现 canter 的 hook 适配、会话记录解析器、install/uninstall、status 一项和数据库版本 6；不改 Claude、Codex 的行为，不做真实 canter 实测，不升版本号、不装到真实环境。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读

- `AGENTS.md`（规矩、隐私、编译目录）。
- `docs/任务/P1-27-接入canter.md`：用户要的结果、用户 11-11 定的几件。
- `docs/DESIGN.md`：D16、§6.2“版本 6”、§7.1 `agents.canter.installed`、§8.8“读会话记录”里 canter 那一条、§10.5 整节、§14 R28–R30。设计主控已在 main 改好，以它为准。
- canter 的公开接口（只读，不改那个仓库）：`~/Developer/personal_projs/canter/docs/DESIGN.md` 的「钩子（P2-153）」「会话记录的公开读取约定（P2-281）」两节。只读这两节；canter 的代码、`~/.canter` 下任何东西都不读。
- 代码：`crates/cairn/src/hook.rs`（`Agent`、`Metadata`、`dispatch`）、`turn.rs`（`Agent`）、`install.rs`（`Paths`、`merge`）、`status.rs`、`transcript.rs`（`extract`、`claude`、`codex`）、`store.rs` 和 `store/schema.rs`（版本升级）、`cli.rs`。测试写法参考 `crates/cairn/tests/` 下的 `hook.rs`、`install.rs`、`transcript.rs`、`store.rs`。

## 在哪里干活

- worktree：`../cairn-worktrees/p1-27-canter`，分支 `p1-27-canter`（已从 main 建好）。
- 编译目录：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target/p1-27-canter`。

## 要做的

照 DESIGN（D16、§10.5）：

- `cairn hook canter`：认 SessionStart、UserPromptSubmit、Stop、SessionEnd，读的字段和输出见 §10.5 的表；来源是 `canter:<session_id>`；没有回合标识，turn_key 留空。输出只用 JSON 的 `hookSpecificOutput.additionalContext`。
- 子会话：stdin 带 `agent_id` 的事件一律不处理：不记事件、不记 `hook_seen`、不注入、不起进程，退出 0、不输出。
- 会话记录解析器 `canter`：只读 §8.8 里写的两种行（用户消息、模型回复），其余行跳过；回退、撤回不处理。和另外两家一样从上次读到的位置往后读，“没什么可写”“解析不了”的规则照旧。
- `cairn install|uninstall --agent canter`：配置在 `$CANTER_HOME/config.json`，没设就是 `~/.canter/config.json`。照 §10.5“安装”：先预览、确认才写、先备份；只合并和删 cairn 自己的条目，别的内容原样保留；解析不了的文件拒绝；配置目录不存在就拒绝、给一行原因，不建目录；目录在、文件不在就新建只有 `hooks` 的文件；不写 `timeout`；不加放行 `save` 的规则。
- `cairn status`：文字和 `--json` 都加 canter 一项（`agents.canter.installed`、`agents.canter.last_seen` 四个键）。没装、没有配置目录或文件、配置读不了都是没装，不报错，另外两家的字段和改前一样。
- 数据库版本 6：照 §6.2 重建 `hook_seen`，放宽 `agent` 约束加上 `canter`，旧行原样保留。只读打开照旧不升级，`status` 读版本 5 的库照常。

## 怎么算做完

用户要的结果（照抄需求单）：

1. canter 成为 cairn 的第三家 agent，和 Claude Code、Codex 一样：在已采用的项目里开 canter 会话，开局注入摘要和项目的账；用户发话时照旧补注入；会话之后由旁路模型写摘要、记账；`cairn status`、`cairn list` 里能看到 canter 的来源。
2. `cairn install|uninstall --agent canter` 装卸 canter 的钩子，和另外两家一样先 `--dry-run` 看、确认才写。
3. canter 是可选的：没装 canter、没有它的配置目录、canter 服务端没在跑、canter 版本太旧，都不影响 cairn 对 Claude Code、Codex 的工作，不报错；`cairn status` 里 canter 照实显示没装，其余字段不变。
4. Claude Code、Codex 的行为照旧。

用户另加的两条：

5. 没装 canter 的机器上，现有测试全过，`status --json` 里 claude、codex 两项和改前一样。
6. 在 canter 子会话里打字，不产生新来源。

验证只做这些：
- 针对这件活的测试：用合成的 canter 钩子 JSON 和合成的 `events.jsonl` 夹具（照 canter 公开约定写），在隔离目录（临时 `HOME`、`XDG_STATE_HOME`、`CANTER_HOME`）里测 hook、解析器、install/uninstall、status；边角带上：版本 5 的库升到 6 后旧的 `hook_seen` 行还在；canter 配置里有别的内容时原样保留；没有 canter 配置目录时 `status` 正常；带 `agent_id` 的事件什么都不写。
- 标准检查跑一次：`cargo test --all-targets`、`cargo clippy --all-targets -- -D warnings`。
- 觉得不够，在回复里说，不要自己加。

## 不要做

- 不改 canter 仓库；不读 `~/.canter` 下任何文件，不读本机真实的会话记录、真实的 cairn 数据库。测试只用隔离目录。
- 不写用户真实配置（`~/.claude/settings.json`、`~/.codex/hooks.json`、`~/.codex/config.toml`、`~/.canter/config.json`），不运行真实的 `cairn install`。
- 不开真实的 canter、Claude、Codex 会话；旁路模型用假的 `codex`。
- 不处理回退、撤回；不读工具调用、工具结果等不在 canter 公开约定里的行；不接 StopFailure、Interrupt。
- 不改 Claude、Codex 的 hook 定义文本和行为；不改别的已有公开约定字段。
- 不升版本号，不改 `HANDOFF.md`。
- 不要按项目名或路径批量杀进程（`pkill -f cairn` 这类）：主控和别的 agent 的进程命令行里都带着项目名和工作目录。停自己起的进程用起的时候记下的 PID。
- 碰到要拿主意的事（包括设计里没写清、或照设计做会和现有行为冲突的地方），停下来，在回复里写清问题、可选办法和你的建议，然后结束这一轮，等主控回复再接着做；不在自己窗口里问用户，不开着命令等，不要自己换别的办法。主控的决定写进完成记录。
- 不合并到 main，不推送。只在 `p1-27-canter` 上提交。

## 做完

在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事、踩过的坑，各几句话。踩过的坑指试了不行、换了办法的，工具或环境的行为和预想不一样的；没有就写「没有」，不要凑。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。
