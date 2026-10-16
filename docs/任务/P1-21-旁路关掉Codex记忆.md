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

## 完成记录

- 做了什么：旁路 `codex exec` 增加固定参数 `--disable memories`；更新 `fake_codex`，缺少该参数或参数值不是 `memories` 时失败。
- 验证了什么：先只改假 `codex`，`cargo test --test sidecar` 按预期因缺参数失败；再改 `generate` 后该测试 25 个全部通过。随后 `cargo test --all-targets --no-fail-fast` 和 `cargo clippy --all-targets -- -D warnings` 均通过。
- 拿主意的地方：沿用现有参数数组，只插入设计文档 §8.8 已规定的两个参数，其余命令参数保持不变。
- 没做的事：未调用真实 `codex`，未安装或修改真实配置，未读取真实会话、记忆或数据库，未改版本号、hook 定义，未合并或推送。
- 主控决定：没有需要主控决定的事项。

## 主控审查

2026-10-16，cairn/main。通过，已合并。diff 只在 `generate` 的参数里插了 `--disable memories`，假 `codex` 改成缺这个参数就失败，别的没动。在 `.target/p1-21-sidecar-memories` 重跑 `cargo test --all-targets --no-fail-fast` 201 项全过、clippy 通过。真 `codex exec --help`（经 IP Guardian 包装脚本）有 `--disable <FEATURE>`，主控派活前已看过。
