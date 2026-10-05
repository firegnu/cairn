# 交叉审查：2d-渲染

2026-10-05，cairn/main（主控）交给 cairn/dev-review-render（Codex，重：gpt-6-astra / xhigh）。
类型：调研
提示：区分已查证事实、推测和未知，给出直接依据；结论限定在本轮调查范围。
你是被委派的审查者：只读审查，不要开别的 agent。**本文件在主仓库工作区（`/Users/firegnu/Developer/personal_projs/cairn/docs/tasks/`），不在你的审查 worktree 里；你只写本文件这一个文件。**

## 背景
- cairn/dev-render（Codex）在分支 `p2d-render` 上完成任务，提交 `6c9be22`。任务书 `docs/tasks/P2d-渲染.md`（审查 worktree 里也有，末尾有完成记录）。
- 主控已审过：没动地基模块和 DESIGN；重跑 `cargo test --all-targets`、clippy、`git diff --check` 通过。
- 另一个并行任务（2d / 2e）在别的分支，不在本次审查范围。

## 先读
- `AGENTS.md`（规矩一节）；任务书与完成记录；任务书"先读"里列的 DESIGN 章节。

## 要审查的
- 审查 worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-p2d-render`（detached，指向 `6c9be22`）。改动范围：`git diff main...HEAD`。**只读：不改、不提交、不切分支。**
- 可以跑该分支的测试和 clippy（命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`）；核实具体怀疑时可在临时目录写一次性 Rust 程序，不留在 worktree 里；暂存区可信根一律指向临时目录。
- 不要运行 claude、codex，不要开 agent；不要读真实 `~/.local/state`、真实会话记录、工具记忆、Corral 事件文件。不用 Python。只做定点核实，不做覆盖矩阵。

## 重点看
1. 渲染是否照 DESIGN §9.1 的六部分与顺序、§9.2 抬头原文；可见性规则（取代、撤回、更正、恢复、删除）有没有算错，会不会把被取代或撤回的记录注入出去，或者漏掉该注入的。

## 输出
追加到本文件末尾「## 审查意见」：先写一句结论（**可以合并** / **改完再合并**）；每条意见写级别（必须改 / 建议改 / 可以不改）、位置、问题、改法。**"必须改"只给两种情况**：任务书"怎么算做完"没达到；或者碰到的要害上有真实缺陷（会丢数据、并发出错、隐私泄露、核心规则算错）。其余一律"建议改"。能给复现步骤的写上。最后对完成记录里"拿主意的地方"逐条表态。

## 回复
回复里只写结论和各级别条数。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 审查意见

**改完再合并。**

2026-10-05，cairn/dev-review-render（Codex）。审查对象为 detached `6c9be22d6224be17438b48a1bf83761c1c350126`，范围为 `main...HEAD` 的 6 个文件。以下共 **必须改 1 条、建议改 1 条、可以不改 7 条**；后 7 条是对完成记录中取舍的逐项表态。

### 1. 必须改：其他工作线注入了原说法，却漏掉已有的可见更正

- **位置**：`crates/cairn/src/render.rs:309–327`；对照同文件 `189–231` 的本工作线更正处理，以及 DESIGN §8.5、任务书“有更正时，在原记录后附最新一条更正，标明来源和时间”。
- **已查证事实**：其他工作线的摘要直接读取 checkpoint 的 `body`，不查找其 correction。构造原记录“旧说法：接口已验证”及稍后的可见更正“更正：接口尚未验证”，实际调用 `session::start(Startup)`，输出仍只有旧说法，完全没有更正内容、来源和时间；`record_ids` 为 `["other-base"]`，`injections` 也只有该原记录。测试未制造预算压力，输出远小于 6,000 字符。这是更正规则漏用到另一条注入路径，后续会话会收到缺少更正的历史结论，属于核心规则缺陷。
- **复现**：在隔离目录初始化 Git 仓库并 adopt；向该项目另一条历史工作线插入 `other-base`（checkpoint，12:01，正文 `## 停点\n旧说法：接口已验证`），再插入 `other-fix`（correction，target=`other-base`，12:02，正文 `## 停点\n更正：接口尚未验证`）；从当前工作线以新来源 startup。探针断言 `out.text.contains("更正：接口尚未验证")` 实际失败。使用的是与分支现有测试相同的合成入库方式和公开 SessionStarted 入口，不依赖尚未实现的 correct CLI。
- **改法**：其他工作线摘要也应查找最新可见更正，在展示原记录摘要时一并体现更正内容、来源与时间，纳入同一预算处理；实际输出的更正内容同步计入 `record_ids` / `injections`。补一条上述定点回归，避免只测试本工作线的更正路径。

### 2. 建议改：常见 Markdown 空行使其他工作线的停点摘要为空

- **位置**：`crates/cairn/src/render.rs:110–116、316–319`。
- **已查证事实**：`stopping_point()` 保留标题后的空行，摘要再直接取 `.lines().next()`。正文为 `## 停点\n\n应出现在摘要里的停点\n\n## 下一步（建议，非授权）\n继续` 时，输出变为 `… · main · 59 分钟前 ·  · 已不可定位，只作历史`，有内容的停点没有出现。该正文符合现有 save 格式校验；没有预算压力，也没有不可见操作。
- **复现**：沿用第 1 条的隔离夹具，只插入一条其他工作线 checkpoint，正文改为上例，调用 `render`，断言输出包含“应出现在摘要里的停点”，实际失败。
- **改法**：仅在提取一行摘要时跳过开头的空白行，取首个非空内容行；完整正文和“停点一节”的渲染继续保留原文。此项是摘要可读性问题，本轮不作为阻止合并的核心规则缺陷。

### 核实范围与验证

- 抬头与 DESIGN §9.2 原文一致；六部分的组装顺序符合 §9.1。现有固定样例逐字比对通过。本工作线先过滤删除、取代、撤回，再选各来源最新 checkpoint；restore 按时间及 ID 撤销较早操作，之后的新操作仍可生效。对本轮读到的这些路径，未发现被删除、被取代或被撤回的 checkpoint 正文漏入注入文本；这不包含第 1 条指出的“更正遗漏”。
- 独立前台运行 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test -p cairn --test render_session`：10 项全部通过。`cargo clippy --all-targets -- -D warnings`（同一 target 设置）和 `git diff --check main...HEAD` 通过。未重复主控已跑过的全量测试，不把主控的结果写成本人重跑结果。
- 两个一次性 Rust 探针位于 `/tmp/cairn-review-render.jRasdE/tests/probe.rs`，复用该提交的测试夹具；命令为 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --offline --manifest-path /tmp/cairn-review-render.jRasdE/Cargo.toml --test probe review_ -- --nocapture`。两项均编译成功，在上述目标行为断言处失败，退出码 101；不是语法或夹具错误。夹具使用临时 HOME / XDG_STATE_HOME、临时 Git 仓库和显式临时暂存可信根，数据随夹具释放。
- 本轮只做代码核对、现有相关测试和两个定点探针，没有覆盖矩阵，没有测试真实 hooks / agent，也没有审查并行 2e 或未来 2f。未读真实会话、工具记忆、真实数据库或 Corral 事件；未修改 worktree、切分支或提交，只追加本审查文件。以上缺陷是已复现事实；对未运行的接入路径不作通过结论。

### 完成记录“拿主意的地方”逐项表态

| 编号与级别 | 位置／取舍 | 问题判断与改法 |
|---|---|---|
| 3. 可以不改 | `render.rs:51–54、266–271`：删除记录完全隐藏，不显示墓碑 | 同意。任务书明确允许二选一；当前也没有通过折叠提示重新显示删除记录。保持。 |
| 4. 可以不改 | `render.rs:13、15–20`：默认 6,000 字符，库级请求可传预算 | 同意。符合首版预算和可配置常量要求，不必在本任务扩展 CLI 配置。保持。 |
| 5. 可以不改 | `render.rs:329–365`：从旧到新，完整正文→停点→引用；仍放不下再舍弃较早其他线摘要、折叠提示和来源引用，保留本线未显示来源数及查看命令 | 同意该有限预算下的取舍。现有预算测试验证降级顺序和计数，未见必须改变的规则。第 1 条修复后的更正内容也需参加预算处理。 |
| 6. 可以不改 | `render.rs:162–164、351–365`：不截断 UTF-8、抬头及现场对比；自定义预算连固定内容也容不下时返回错误 | 同意。固定规则不宜截成残句；没有把错误转换成伪造的成功输出。保持。 |
| 7. 可以不改 | `render.rs:215–262、326、375–387`，`session.rs:82–89`：只有实际带内容的 checkpoint、更正和其他线摘要计入 injections，纯引用与折叠提示不计 | 同意这一计数原则；现有测试也验证纯引用下次 resume 仍可补入。第 1 条是摘要漏更正，须修复后让正文与 ID 列表继续一致，无须另改这一原则。 |
| 8. 可以不改 | `session.rs:55`，`render.rs:95–107`：UTC RFC 3339 毫秒格式、传入时钟计算时长、未来时间显示“不到 1 分钟前” | 同意。沿用 2c 的排序约定，时间可重复；未来时间这一显示近似不构成本轮合并阻碍。保持。 |
| 9. 可以不改 | `tests/render_session.rs:564` 与完成记录中的用户确认：禁用词仅约束自动文字，历史正文原样保留 | 同意。已有用户明确确认，不重开决定；现有定点测试验证自动叙述与正文保留。保持。 |

## 复核意见

**可以合并。**

2026-10-05，cairn/dev-review-render（Codex）。本轮仅复核原第 1、2 条及 `6c9be22..a6c4a56` 的返工差异；审查 worktree 为 detached `a6c4a561e22c63fa669764766d2d70c71a399d21`。**原必须改 1 条、建议改 1 条均关闭；本轮新增必须改 0 条、建议改 0 条、可以不改 0 条。** 上轮第 3–9 条的取舍结论保持，不重新审查。

- **原第 1 条必须改：已修好，关闭。** `render.rs:328–340` 为其他工作线查找最新可见更正，输出完整更正正文、来源和时间，并把更正 ID 加入同一个 `Piece`。原摘要与更正共同参加字符预算，预算不足时整体省略，不单独留下旧说法。重跑上轮原封未动的复现探针，实际输出包含“更正：接口尚未验证”；`record_ids` 为 `["other-base", "other-fix"]`，数据库 injections 为 2 条。新增正式测试也通过，验证排除较早及已撤回的更正，并在缩小预算时同时移除原摘要、更正及相应 ID。
- **原第 2 条建议改：已落实，关闭。** `render.rs:316–319` 仅将一行摘要的取值改为首个非空白内容行。上轮空行复现探针通过；新增正式测试进一步验证跳过仅含空格、制表符的行，并逐字核对摘要和完整正文，未改变完整正文或停点一节的原文。
- **返工差异：本轮未发现新问题。** 差异仅涉及 `render.rs`、`tests/render_session.rs` 和任务书返工记录；更正筛选沿用既有可见性条件，预算组装及 injections 写入继续使用既有路径，原测试未删除、未放宽断言。用户本人对“限定自动生成文字，正文原样保留”的确认已收到，本轮不再质疑或扩大该边界。

本轮独立验证全部在前台等待结束，Cargo 均使用指定共享 `CARGO_TARGET_DIR`：

- `cargo test -p cairn --test render_session`：12 项通过。
- `cargo test --offline --manifest-path /tmp/cairn-review-render.jRasdE/Cargo.toml --test probe review_ -- --nocapture`：上轮两个失败探针均通过，未修改探针、未新增核实场景。
- `cargo clippy --all-targets -- -D warnings` 与 `git diff --check 6c9be22..a6c4a56`：通过。

未重跑全量测试，不把开发方的全量通过记录当成本人重跑结果。核实仍只使用合成材料、临时 HOME / XDG_STATE_HOME、临时 Git 仓库和显式临时暂存可信根；没有运行真实 agent、读取真实数据或扩展到并行任务。未改代码、测试或审查 worktree，未切分支、未提交，只在本文件追加复核意见。结论限定于上述两条修复及本次提交差异。
