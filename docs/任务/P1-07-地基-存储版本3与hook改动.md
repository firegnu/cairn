# 任务：旁路摘要的地基——存储升到版本 3，hook 不再续跑、改为记会话，save 版本顶替，注入布局和抬头改新

2026-10-15，cairn/main 交给 cairn/dev-schema-v3（Codex，重：gpt-6-astra / xhigh）。
路由：重 / 交叉审查要 / 影响面：碰要害（路由：重，要，碰要害）
类型：功能变更
依据：本轮做旁路摘要的地基：数据库版本 3、四个 hook 的新流程（不含起等待进程和补注入）、save 的版本顶替、注入布局和抬头。不做读会话记录、调旁路模型、等待进程、`cairn summarize`、`cairn config`，这些是后面的任务。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 用户原话

- “我发现一个问题，不会是每一个对话都会被记录吧？因为我发现现在已经记录了7跳了，短短8分钟”
- “存的太频繁会导致两个问题，1: cairn的记录太多 2: 存的太频繁会造成模型存这个的也频繁感觉会耗费模型token也会感觉变慢。”
- “cairn save可以留着，但是不像现在这么调用。”
- 2026-10-15：“现在的方案我测试的几个repo，记录的都太密集了”

## 先读

- `AGENTS.md`。
- `docs/DESIGN.md`：开头两段修订说明；§2 的 D6、D9；§6.2 的“版本 3”一段；§7.1（公开约定，只加不改）；§8.1；§8.2 第 9 步；§8.3；§8.4；§9.1；§9.2；§9.3；§11.1；§14 的 R7。
- `docs/背景与决策记录.md` §3.9（为什么这样改，不用细读）。

## 在哪里干活

- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p1-07-schema-v3`，分支 `p1-07-schema-v3`（已从 main 建好）。
- 编译目录：命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。
- 并行任务：cairn/dev-transcript 在分支 `p1-08-transcript` 新建 `crates/cairn/src/transcript.rs` 和对应的测试、夹具，并在 `lib.rs` 加一行 `mod`。你不要动这些文件；`lib.rs` 你要改也只加自己的行。它会用 `crate::turn::Agent`（`Claude`、`Codex` 两个值），这个枚举的名字、位置和取值不要改。

## 要做的

照 DESIGN 的上述章节实现，要点：

1. **存储版本 3**（§6.2）：`records` 加 `written_by`，加 `sessions` 表，列照 §6.2 草案（等待进程那两列 `waiter_id`、`waiter_beat_at` 一并建上，本轮不用）。升级办法照版本 2：读写方式打开时在一个立即事务里补上，只读方式打开不升级，只读的一方（`status`、`list --json`）遇到版本 2 的库照样能用。已有的表和数据不动，已有记录都算 `save` 写的。
2. **SessionStarted**（§8.1）：第 3 步在 `sessions` 里记来源的工作线、会话记录路径、当时的 Git 事实。第 4 步（起等待进程）本轮不做，留空。
3. **TurnStarted、TurnEnded**（§8.3）：回合结束不再判定确认、不再续跑，一律放行、不输出；按 §8.3 更新 `sessions`（路径在回合开始时就记，Git 事实在回合结束时采）。TurnStarted 第 4 步（补注入）和 TurnEnded 第 5 步（起等待进程）本轮不做。`turn_decisions` 不再读写，表留着。
4. **SessionEnded**（§8.4）：只记事件，本轮不起等待进程。
5. **save 的版本顶替**（§8.2 第 9 步）：同一来源在同一工作线上已有可见 checkpoint 时，新记录顶替它。
6. **注入**（§9.1、§9.2、§11.1）：最近 3 个来源给全文并标明谁写的，更早的来源每个一行、最多 10 行；同一来源的版本更替不进折叠提示；抬头换成 §9.2 的新文字；“之后观测到的事件”不再报“回合结束时没有确认”。§11.1 里和摘要有关的几条（还没有摘要、正在写、`cairn summarize`）本轮不做。
7. **公开约定**（§7.1）：`list --json`、`show <ID> --json` 加 `written_by`。其余已列字段不改。

修订前的行为（续跑、确认窗口）是有意去掉的。测它们的测试改成断言新行为，不是删掉了事；改了哪些写进完成记录。

## 怎么算做完

- 上面 7 条照 DESIGN 做到。
- 验证只做这些：每条新行为至少一条针对它的测试；版本 2 升到版本 3 的迁移测试（旧数据都在、再开不再变、只读打开版本 2 的库不升级、两个连接同时升级）；`cargo test --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`git diff --check` 各跑一次。觉得不够，在回复里说，不要自己加。

## 不要做

- 不做读会话记录、调旁路模型（`codex exec`）、等待进程、补注入、`cairn summarize`、`cairn config`。
- 不改 hook 的定义和 `install`、`uninstall` 写进配置的内容（§10.3：这次修订不改 hook 的定义）。
- 不改 `docs/DESIGN.md`。实现中发现设计说不通、或两处矛盾，停下来报告，等决定，不要自己换做法。
- 不碰真实数据：不读本机真实的会话记录、各工具的记忆文件、用户真实的 cairn 数据库（`~/.local/state/cairn/`）。测试只用临时 `HOME`、`XDG_STATE_HOME` 和临时 Git 仓库。
- 不要写 `~/.claude/settings.json`、`~/.codex/hooks.json`、`~/.codex/config.toml`，不运行真实的 `cairn install`，不 `cargo install`。
- 不要按项目名或路径批量杀进程（`pkill -f cairn` 这类）：主控和别的 agent 的进程命令行里都带着项目名和工作目录。
- 不合并到 main，不推送。只在 `p1-07-schema-v3` 上提交。

## 做完

在本文件末尾追加“## 完成记录”（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。
