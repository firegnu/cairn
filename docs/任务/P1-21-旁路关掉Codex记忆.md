# 任务：旁路调 `codex exec` 时带上 `--disable memories`

2026-10-16，cairn/main 交给 cairn/dev-sidecar-mem（Codex，轻：gpt-5.6-luna / medium）。
路由：轻 / 交叉审查不要 / 影响面：改行为（路由：三项都拿不准，档位“常规” 0.86；推翻：只是给一条已有命令加一个固定参数、改一处测试夹具，机械改动，定为轻）
类型：功能变更
依据：本轮只给旁路调用加这一个参数并让测试守住它，不改别的参数、不改版本号。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要开别的 agent。

## 用户原话

2026-10-16，主控指出旁路的 `codex exec` 没带 `--disable memories`、本机 Codex 记忆开着，可能把别的项目的内容带进摘要和账；用户：“补吧”。

## 先读

- `AGENTS.md`（规矩、编译目录、隐私）。
- `docs/DESIGN.md` §8.8“调旁路模型”（2026-10-16 加了 `--disable memories`）。
- `crates/cairn/src/sidecar.rs` 的 `generate`；`crates/cairn/tests/sidecar.rs` 的 `fake_codex`。

## 在哪里干活

- worktree：`../cairn-worktrees/p1-21-sidecar-memories`，分支 `p1-21-sidecar-memories`（已从 main 建好）。
- 编译目录：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target/p1-21-sidecar-memories`。

## 要做的

- `generate` 调 `codex exec` 时带上 `--disable memories`（会话摘要和记账都走这里）。其余参数不变。
- 假 `codex` 要求收到 `--disable memories`，没收到就失败。

## 怎么算做完

- 旁路摘要和记账调 `codex exec` 时都带着 `--disable memories`。
- 验证只做这些：先改假 `codex`，跑 `cargo test --test sidecar` 看到因为缺这个参数而失败；再改 `generate`，跑通；最后 `cargo test --all-targets --no-fail-fast` 和 `cargo clippy --all-targets -- -D warnings` 各一次。觉得不够，在回复里说，不要自己加。

## 不要做

- 不改别的参数，不改版本号，不改 hook 定义。
- 不调真的 `codex`，不安装到真实环境，不改真实配置；不读本机真实的会话记录、各工具的记忆文件（包括 `~/.codex/memories` 下的任何文件）和真实的 cairn 数据库。
- 不要按项目名或路径批量杀进程（`pkill -f cairn` 这类）。
- 不合并到 main，不推送。只在 `p1-21-sidecar-memories` 上提交。

## 做完

在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。
