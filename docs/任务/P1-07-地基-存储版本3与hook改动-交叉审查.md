# 交叉审查：P1-07 地基（存储版本 3 与 hook 改动）

2026-10-15，cairn/main 交给 cairn/dev-review-schema-v3（Codex，重：gpt-6-astra / xhigh）。只读审查。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 背景

cairn/dev-schema-v3 在分支 `p1-07-schema-v3` 做了旁路摘要的地基：数据库升到版本 3（`records.written_by`、`sessions` 表）；四个 hook 不再判定确认和续跑，改为记会话状态；`save` 同一来源在同一工作线上的新记录顶替上一条；注入改成最近 3 个来源全文、更早的每个一行，抬头换新。主控已看过 diff，并用单独的编译目录重跑了全量测试和 clippy，全部通过。主控的审查结论在任务书末尾。

## 先读

- `AGENTS.md`。
- 任务书 `docs/任务/P1-07-地基-存储版本3与hook改动.md`（含完成记录和主控审查）。
- `docs/DESIGN.md`：§6.2“版本 2”“版本 3”两段；§7.1；§8.1；§8.2 第 9 步；§8.3；§8.4；§9.1；§9.2；§10.2 表格里 Stop 一行；§11.1。

## 要审查的代码

- 你的 worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-p1-07-schema-v3`（detached，就是 `p1-07-schema-v3` 的最新提交）。改动看 `git diff main...p1-07-schema-v3`。
- **只读**：不改代码、不提交、不切分支。唯一要写的文件是本文件（在主仓库 `/Users/firegnu/Developer/personal_projs/cairn/docs/任务/` 下，不在你的 worktree 里）。
- 可以跑测试，编译目录用单独的：命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target-review-p1-07b`（共用的那个会被并行任务覆盖）。真怀疑某一处，可以在临时目录用合成输入实测那一条；测试一律用临时 `HOME`、`XDG_STATE_HOME`、临时 Git 仓库。
- 不读本机真实的会话记录、各工具的记忆文件、用户真实的 cairn 数据库（`~/.local/state/cairn/`）；不运行真实的 `cairn install`，不写 `~/.claude`、`~/.codex` 下的配置。
- 验证照“碰要害”这一档：读代码、跑相关测试、挑可疑处实测。不做覆盖矩阵，不录屏。

## 重点看

1. 迁移：版本 1、2 的库升到 3 时旧数据和 rowid 是否都在、失败能否整体回滚、并发打开是否安全；只读打开（`status`、`list --json`）遇到版本 2 的库是否照样能用、不升级。
2. 版本顶替（`ingest.rs`）：只顶替同来源、同工作线、可见的 checkpoint；和显式 `--supersedes`、撤回、更正、`restore` 放在一起时，可见性和 `replaced_by` 是否还对；同一来源两条 save 同一次收取时顺序是否对。
3. `sessions` 的写入（`session.rs` 的 UPSERT）：事件乱序、并发时 `turns`、`last_event_at`、路径、分支、`facts` 是否按 §8.3 的意思保留较新的；`failures` 只在回合开始时清零；采不到 Git 事实时保留上次的。
4. hook 的行为：任何情况都不再 block、续跑；禁用、未采用、出错时放行且不写库；会话记录只记路径、从不打开；Codex Stop 改为空输出是否和 §10.2 一致。
5. 注入：最近 3 个来源全文、更早来源最多 10 行、预算不够时的降级；`injections` 和 `show --json` 的 `record_ids` 是否只包含实际出现在文字里的记录（§7.1）；同一来源的版本更替不进折叠提示。

## 输出

追加到本文件末尾“## 审查意见”。先写一句结论（可以合并 / 改完再合并），然后每条意见写级别（必须改 / 建议改 / 可以不改）、位置、问题、改法，最后对完成记录里“拿主意的地方”逐条表态。
**“必须改”只给两种**：任务书“怎么算做完”没达到；上面重点看的地方有真实缺陷（会丢数据、并发出错、泄露、核心规则算错）。其余一律“建议改”。

不要按项目名或路径批量杀进程（`pkill -f cairn` 这类）。

## 回复

只写结论和各级别的条数，命令都在前台跑完，最后一行写 DONE。

## 审查意见

改完再合并：必须改 1 条，建议改 0 条，可以不改 0 条。

2026-10-15，cairn/dev-review-schema-v3。审查提交 `d619312498da54a88fb735ffe8d89c9898dfcf9e`，比较基线 main 为 `607959c34b4dd2e745e4dfdd2f61d7edfc4fe38e`。只读检查代码，未改代码、提交或切分支。

### R1 — 必须改：较早来源有更正时，把未展示内容的原记录登记成已注入

- **位置**：`crates/cairn/src/render.rs:221–247`，尤其是 221 行的 `correction.unwrap_or(record)` 与 227–230 行无条件登记未注入原记录的组合；登记落库在 `crates/cairn/src/session.rs:108–113`，resume 跳过逻辑在 `render.rs:205–216`。
- **问题**：第 4 个及以后的来源若有更正，一行文字只采用更正的停点，原 checkpoint 只剩来源和 ID 引用，没有展示它自己的正文或停点；但 `record_ids` 同时包含原 checkpoint 和更正，SessionStart 因此把两条都写入 `injections`。这与本模块 `Rendered.record_ids` 的“只有实际展示内容才登记、纯引用不算”规则不一致，也破坏本次重点第 5 条要求的注入追溯。后续 resume/fork 会认为原记录已经展示过而跳过；`--supersedes` 的注入条件也会把它认作已注入对象（`ingest.rs:270–273`）。
- **合成实测**：在临时 HOME / XDG_STATE_HOME 和临时 Git 仓库中建立 4 个来源，原记录的停点分别为 `ORIGINAL-0` 至 `ORIGINAL-3`，用正常 `correct record-0` 写入 `CORRECTED-0`。SessionStart 显示的一行是 `来源 codex:source-0 · 记录 record-0 · 更正 <更正ID>（来源 local:<更正ID>） · CORRECTED-0`；`ORIGINAL-0` 不在文字里，`injections` 却已有 `record-0`。`show --json` 的 `record_ids` 为 `record-3, record-2, record-1, record-0, <更正ID>`；随后 resume 的 `record_ids` 为空。最后的 `--supersedes` 影响由校验代码确认，未另做实测。
- **改法**：让一行来源摘要与登记的 ID 对齐。最小且保持原有更正展示语义的做法，是在同一行内分别保留原停点第一行与标明来源/ID 的更正停点第一行，再登记实际展示了内容的两条。若选择只显示更正，则不能仅凭原 ID 引用登记原记录，并须处理后续增量注入，避免又单独补出未经更正的原内容。补一条“第 4 个来源带更正”的回归，同时核对文字、`show --json.record_ids`、`injections` 和 resume；预算移除整行时，两条登记也都应移除。

### 已核对与验证边界

- 版本 3 迁移在同一个立即事务里追加列和表；版本 1/2 的只读打开不迁移，旧记录的作者按 `save` 读取。现有测试证明旧表数据和 rowid 保留、再开幂等、版本 1 升级失败整体回滚、版本 2 并发升级成功或返回可重试的忙错误。没有拿真实数据库试升级。
- 自动版本顶替的候选限定为同项目、同来源、同工作线、可见 checkpoint，显式取代的校验仍在入库前完成。额外合成实测通过：同来源两条 save 同次收取按 ULID 文件顺序建立版本关系；显式取代与自动顶替指向同一记录时只留一条关系；撤回、恢复新版本以及恢复被取代版本后，`retracted` / `replaced_by` 保持现有语义。
- 会话 UPSERT 按事件时间保护工作线、路径、分支和 `last_event_at`；`turns` 原子累加，`failures` 只在 TurnStarted 清零。事实按采集时间保留较新快照，采集失败保持上次事实和分支。现有乱序、并发、采集失败用例通过。SessionEnded 按本任务只写事件及 hook_seen，不更新 sessions。
- 两家 Stop 都不再判定确认或返回 block；Codex 空输出符合 DESIGN §10.2。禁用、未采用、数据库/解析失败的放行边界通过；Stop 收取或采事实失败后仍保存会话状态，是 §8.3 明确要求的例外，不作为“失败时不写库”的缺陷。路径仅作为元数据保存，FIFO 合成夹具证明未打开会话记录。TurnStarted 新增取分支的 Git 调用复用既有超时封装，不采完整事实。
- 最近 3 个来源全文、更早最多 10 行、作者标注、预算逐段降级和同来源版本不进折叠提示的现有测试通过；R1 是新增的一行更正路径与注入登记的组合缺陷。
- 在指定独立目录 `.target-review-p1-07b` 前台跑完 `cargo test --all-targets`，126 项全部通过（cmds 21、hook 14、install 13、render_session 15、save 19、scope_facts 5、store 23、turn 16）；`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`git diff --check main...p1-07-schema-v3` 均通过。
- 两个临时探针在 `/tmp/cairn-schema-v3-review.ZRNs2h/probe.rs`，只调用当前提交的库和既有隔离夹具；`probe review_ --nocapture --test-threads=1` 两项完成，第一项断言并打印上述错误登记的实际状态，第二项验证版本组合。未读真实会话记录、真实 cairn 数据库，未运行真实 install 或改真实配置；未做覆盖矩阵或录屏。

### 对“拿主意的地方”逐条表态

1. **同来源顶替沿用可见性和最新排序、不回填历史、显式与自动关系去重：同意。** 符合 §8.2 第 9 步和版本 3 保留旧数据的边界；不把恢复出来的旧记录另行批量隐藏。
2. **单条 UPSERT；旧事件仍计开始次数但不覆盖较新元数据；事实按采集时间保留：同意。** 与 §8.3 不依赖回调串行、开始次数是观测计数的约定一致，计数和元数据并发保护均有现有测试。
3. **先删最早的一行来源、再降级最近正文，遗漏计数保留：同意。较早来源有更正时只取更正停点：需按 R1 调整。** 单行显示更正且标明更正来源/ID的方向可保留，但不能把没有展示内容的原记录同时登记为已注入。

## 复核

可以合并：必须改 0 条，建议改 0 条，可以不改 0 条。

2026-10-15，cairn/dev-review-schema-v3。仅复核 `d619312..98eef61` 的 R1 修复及直接回归；当前 detached HEAD 为 `98eef61cffb3cbe50b5885bafc27aa1f0afc9d9c`，未重审整个分支。

- **R1 已修复**：`crates/cairn/src/render.rs:221–263` 按主控选择，在同一行先显示原停点第一行，再显示带来源和 ID 的更正停点第一行；`record_ids` 顺序与内容出现顺序一致。原记录已注入时，增量路径不再展示或登记原内容，只处理未注入的更正。两者仍在同一个 `Piece` 中，预算移除整行时，文字和两条 ID 一起移除。
- **直接回归通过**：新增 `fourth_source_correction_content_and_injections_stay_together_through_budget_and_resume` 核对正常显示、跳过停点开头空行、仅取第一行、预算移除，以及 `show --json.record_ids`、实际 `injections` 和 resume 的一致性。正常显示后 resume 不重复；整行被预算移除后 resume 能补入原记录和更正，再次 resume 不重复。现有来源数量、预算降级和增量更正测试也通过。未发现本次修复引入的新问题，无新增建议项。
- **实际验证**：使用 `.target-review-p1-07b`，前台跑完 `cargo test --test render_session --test hook --test cmds`：render_session 16、hook 14、cmds 21，共 51 项全部通过；`git diff --check d619312..98eef61` 通过。测试使用既有隔离夹具；未改代码、提交或切分支，唯一仓库写入为本节追加。

上一轮“拿主意的地方”第 3 条中更正摘要的保留意见随 R1 关闭；其余上一轮结论保持不变。
