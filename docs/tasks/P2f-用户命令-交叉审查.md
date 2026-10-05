# 交叉审查：2f 用户命令

2026-10-05，cairn/main（主控）交给 cairn/dev-review-cmds（Codex，重：gpt-6-astra / xhigh）。
类型：调研
提示：区分已查证事实、推测和未知，给出直接依据；结论限定在本轮调查范围。
你是被委派的审查者：只读审查，不要开别的 agent。**本文件在主仓库工作区（`/Users/firegnu/Developer/personal_projs/cairn/docs/tasks/`），不在你的审查 worktree 里；你只写本文件这一个文件。**

## 背景
- cairn/dev-cmds（Codex）在分支 `p2f-cmds` 上实现了 list、show <ID>、correct、retract、restore、delete、export，提交 `cd8ac7b`。任务书 `docs/tasks/P2f-用户命令.md`（审查 worktree 里也有，末尾有完成记录）。
- 主控已审过：没动禁改模块和 DESIGN；`render.rs` 有小幅抽取，渲染测试仍全过；重跑 `cargo test --all-targets`（83 项）、clippy、`git diff --check` 通过。

## 先读
- `AGENTS.md`（规矩一节）；任务书与完成记录；任务书"先读"里列的 DESIGN 章节。

## 要审查的
- 审查 worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-p2f-cmds`（detached，指向 `cd8ac7b`）。改动范围：`git diff main...HEAD`。**只读：不改、不提交、不切分支。**
- 可以跑该分支的测试和 clippy（命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`）；核实具体怀疑时可在临时目录写一次性 Rust 程序，不留在 worktree 里；暂存区可信根一律指向临时目录。
- 不要运行 claude、codex，不要开 agent；不要读真实 `~/.local/state`、真实会话记录、工具记忆、Corral 事件文件。不用 Python。只做定点核实，不做覆盖矩阵。

## 重点看
1. **delete 的隐私**：正文真的从库和 WAL 清掉；`CheckpointBusy` 时如实报告未完成且返回非 0；交互确认、`--yes`、非终端时拒绝；删除后 show / list / export / 注入都不再带出正文（包括引用它的更正）。
2. **可见性规则一致**：list、show、export 与注入渲染（2d）对取代、撤回、更正、恢复、删除的判定是否一致；restore 是否只撤销该撤销的；`render.rs` 的抽取有没有改变注入输出。
3. **只追加与来源**：correct / retract / restore 只插入新行不改旧行；来源 `local:<ULID>`、关联不确定；目标不存在、已删除、跨项目时的处理。
4. **export**：不覆盖已有文件（独占创建、不跟随符号链接）、0600、"非权威"抬头、只含可见记录。
5. **只读命令**：库不存在时不建库；执行前收取暂存区的行为与 §6.5 一致。

## 输出
追加到本文件末尾「## 审查意见」：先写一句结论（**可以合并** / **改完再合并**）；每条意见写级别（必须改 / 建议改 / 可以不改）、位置、问题、改法。**"必须改"只给两种情况**：任务书"怎么算做完"没达到；或者碰到的要害上有真实缺陷（隐私正文残留或泄露、核心规则算错、丢数据）。其余一律"建议改"。能给复现步骤的写上。最后对完成记录里"拿主意的地方"逐条表态。

## 回复
回复里只写结论和各级别条数。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 审查意见

**改完再合并**。

2026-10-05，cairn/dev-review-cmds；审查对象为 detached `cd8ac7b947fbf563f338f2ee10f9bcc6c8847a9f`，范围为 `git diff main...HEAD`。共必须改 2 条、建议改 0 条、可以不改 6 条（末尾对实现取舍的逐条判断）。

### 必须改

1. **必须改：成功写入的二级更正不会随原记录显示，注入和导出继续给出被更正的旧说法。**

   **位置**：`crates/cairn/src/commands.rs:149–167`、`:123–137`、`:233–238`；共享显示路径 `crates/cairn/src/render.rs:198–239`、`:394–396`。

   **已查证事实**：`append` 只检查目标存在、未删除，没有限制 correction 的目标 kind。依次创建 checkpoint A、执行 `correct A` 得 C1、执行 `correct C1` 得 C2，三个写入均成功。`show C1 --json` 的 `correction.body` 是 C2，但 `show A`、不带 ID 的 `show`（注入渲染）和 `export` 仍只显示 C1 正文，完全没有 C2。原因是显示原记录时只查询一层 `target_id`，随后直接打印 C1 的正文，没有处理对 C1 的更正。此时旧说法仍作为最新更正交给下一个会话，属于核心更正规则错误，也没有兑现验收第 1 条中更正进入注入的要求。

   **定点复现**：临时 Rust 程序用 `cli::run_at`，显式传临时 cwd、数据库路径与暂存可信根；先 adopt / save A，再以不同合成标记执行上述两次 correct。断言确认 `show C1` 含 C2，原记录详情、注入、导出含 C1 而不含 C2。没有依赖正文预算或来源折叠来触发问题。

   **改法**：使写入入口接受的目标与共享显示能力一致。若支持更正链，显示 C1 时也必须带上其最新可见更正 C2 的正文、来源和时间，并让详情、注入、导出共享处理；若首版只支持更正 checkpoint，需先由主控明确这一设计边界，再在写入前拒绝不支持的目标，不能接受并保存后静默遗漏。补一个上述链条的定点回归即可；涉及既有 render 行为变更时按原任务限制先报主控。

2. **必须改：restore 使用当前墙钟排序，遇到已提交但时间领先的撤回时会报成功却没有恢复。**

   **位置**：`crates/cairn/src/commands.rs:155–169`；其依赖的既有排序规则在 `crates/cairn/src/render.rs:62–76`，时间来源为 `crates/cairn/src/save.rs:23–24`。

   **已查证事实**：在隔离合成库中，已有针对 A 的 retraction，`created_at` 比执行 restore 时的墙钟领先约 2 秒。`restore A` 通过“有可撤销状态”的检查、插入 restore 并返回成功；紧接着 `show A --json` 的 `retracted` 仍为 true。SQL 只接受 `(restore.created_at, restore.id) > (retraction.created_at, retraction.id)` 的恢复，而命令直接使用 `now()`，没有保证新动作在该排序中晚于它刚读到的旧动作。这违反验收第 3 条“restore 之后重新可见”。排序 SQL 来自 2d，但本轮新增的实际写入入口没有保证其排序前提。

   **定点复现与边界**：临时 Rust 程序先经公开入口 save A / retract A，再只在临时库中把该撤回的时间设为 `Utc::now() + 2s`，模拟本机墙钟回拨后的已存记录；随后公开入口 restore A 返回成功，查询仍为撤回。程序没有改变系统时钟。已验证的是上述合成状态下的错误；它对应时钟回拨场景是原因推断，没有声称本机真实发生过回拨。同毫秒的随机 ULID 排序本轮未另做实测，不计作额外发现。

   **改法**：在追加事务内保证恢复动作确实晚于它要撤销的已提交动作，统一处理后续撤回与恢复的动作顺序，不能只把墙钟当作因果顺序。若本轮无法在现有接口内安全处理倒序，至少检测到后回滚并给出明确错误，不能提交无效恢复后报成功；需要改动原任务禁改模块或设计时先报主控。保持旧记录不变，补上述倒序时间的定点回归，不靠 sleep 消除问题。

### 已完成的核实

- `cargo test --all-targets` 前台完成，83 项通过：cmds 10、render/session 12、save 19、scope/facts 5、store 17、turn 16、probe 4。`cargo clippy --all-targets -- -D warnings` 前台完成、通过；均使用规定的共享 `CARGO_TARGET_DIR`。`git diff --check main...HEAD` 通过。
- 既有固定注入文本比对通过；差异核对确认 `render.rs` 的抽取保留原查询、排序和输出路径。上面两条来自新增命令可产生的状态，不是仅凭抽取形式猜测的回归。
- 删除验收测试实际覆盖了独立读事务阻塞 WAL 检查点、非 0 退出、精确未完成提示、提交墓碑、解除阻塞后重试、数据库与 WAL 原正文扫描及首次删除时间保留。交互确认、取消和非终端拒绝测试通过。
- 另在上述一次性 Rust 程序中删除有 C2 引用的 C1：C1 墓碑不带正文或更正；原记录详情、C1/C2 详情、`list --all`、export、注入均没有 C1 的独有正文标记，数据库与 WAL 扫描也没有该标记。C2 的不同正文作为独立历史记录保留。此结论不扩展为“自动删除其他记录里由用户另行复制的相同文字”。
- correct / retract / restore 的写入为 IMMEDIATE 事务中的 INSERT，目标校验在事务内；新来源为 `local:<ULID>`、`uncertain`，没有更新旧 records 行。无效正文、不存在 / 已删除目标、跨项目更正归属的现有测试通过。
- export 的 `create_new(true)` 拒绝已有普通文件和末级符号链接，0600 权限、非权威抬头与工作线过滤测试通过。只读命令缺库时不创建数据库、暂存区或导出文件；有库时复用收取入口后再执行，符合本轮任务及 2d 已有边界。
- 定点程序及其数据只放在临时目录，暂存可信根显式指向临时目录；没有启动真实 agent、访问真实数据库或会话。未修改、提交或切换审查 worktree；仓库内只追加本审查文件。

### 对“拿主意的地方”逐条表态

1. **可以不改：按全局 ID 查看和管理，不受当前项目限制。** 位置：`commands.rs:11–20`。没有发现跨项目误归属；DESIGN §5 明确保留旧项目键下的历史供命令查看，这个选择合理。改法：无，保留显式 ID 定位与现有跨项目测试。
2. **可以不改：追加声明沿用目标的项目、工作线和分支，不重新采集 Git 事实。** 位置：`commands.rs:165–167`。声明针对原记录；从另一个目录更正时，采集调用目录事实反而会混淆对象。原事实保留、新声明 facts 为空已经验证。改法：无。
3. **可以不改：list 展示各 kind，--all 展示隐藏记录及状态。** 位置：`commands.rs:175–205`。list 是记录管理入口，撤回 / 恢复声明可供追溯；与注入只选 checkpoint 并附更正的用途差异有解释。改法：无；不要因此把不受支持的更正目标也默认为有效，见必须改第 1 条。
4. **可以不改：export 保留当前线全部可见 checkpoint，附更正，不套用注入的每来源最新一条和字符预算。** 位置：`commands.rs:225–240`。DESIGN §8.5 要求导出可见记录，没有要求按注入预算裁剪。改法：保留这一导出范围；更正链遗漏按必须改第 1 条修复。
5. **可以不改：delete 只物理清除指定记录正文，不级联删除其他历史。** 位置：`commands.rs:214–221`，调用既有 `Store::delete_body`。符合按 ID 删除正文的设计，避免未经指定删除其他记录；本轮已验证被删正文不会通过引用读取出来。改法：无；不能把它宣传成对所有历史副本的全文清除。
6. **可以不改：墓碑只显示元数据，不附正文、事实或更正。** 位置：`commands.rs:57–75`、`:93–95`、`:123–130`。这符合任务要求的墓碑展示；保留数据库中的原元数据并不意味着必须重新输出 facts。改法：无，保持现有墓碑分支与删除测试。

## 复核意见

**改完再合并**。

2026-10-05，复核 detached `d844513cdecb9140e0a400fe37ec1390e75144e1`，仅检查前次两条必须改及 `git diff cd8ac7b..d844513`。本轮剩余必须改 1 条、建议改 0 条、可以不改 0 条；前次六项实现取舍不重复审查或计数。

### 前次两条的处理结果

- **第 1 条已修好，可以关闭。** 接受主控已经决定并写入 DESIGN §8.5 的边界：correct 只支持 checkpoint，不再讨论更正链。`commands.rs:156–158` 在 IMMEDIATE 事务中、写来源和记录之前拒绝其他 kind。新增回归实际通过：correction / retraction / restore 目标均返回非 0 和精确单行错误，记录与来源不增加；重新更正原 checkpoint 后，详情、注入和导出均显示新更正。
- **第 2 条原复现已修好，但动作顺序尚未闭合。** 新代码在事务内取既有动作最大时间并加 1 ms，能解除时间领先的取代和撤回；之后的 retract / restore 也有效，旧记录保持不变，无法递增时回滚报错。新增回归通过。不过，这个时间推进仅应用于 retraction / restore，随后通过 save 写入的取代不使用同一顺序，形成下面这条必须改。

### 剩余必须改

1. **必须改：抬高时间的 restore 会撤销它之后才写入的新取代。**

   **位置**：本轮改动 `crates/cairn/src/commands.rs:165–188`；直接关联的既有路径为 `crates/cairn/src/ingest.rs:296–305`（新 checkpoint 保留 save 的墙钟时间）和 `crates/cairn/src/render.rs:67–70`（时间更晚的 restore 抵消取代）。

   **已查证事实**：沿用前次的时钟倒序合成条件，restore A 成功且 A 重新可见后，再由一个已被注入 A 的合成来源执行 `save --supersedes A`，新 checkpoint B 成功收取，`spool_ops.outcome='ingested'`，`supersessions(B,A)` 确实存在。但 B 的时间仍来自当前墙钟，早于被提高到未来的 restore 时间，因此旧 restore 把新取代也抵消了：`show A --json` 的 `replaced_by` 为 null，默认 list、注入和 export 仍带出 A。这是实际的核心取代 / 恢复规则错误，不能把“恢复”解释成撤销未来尚未发生的取代。

   **定点复现**：一次性 Rust 程序只使用临时 HOME / XDG_STATE_HOME、显式临时数据库与暂存可信根，步骤如下：

   1. 通过 `cli::run_at` adopt、save A、retract A；仅在合成库中将该撤回的时间改为 `Utc::now() + 2s`，没有调整系统时钟。
   2. 通过公开入口 restore A，核对 `retracted=false`、`replaced_by=null`。
   3. 调用库级 `session::start` 注入 A 给合成来源 `codex:synthetic-p2f-recheck`，断言 A 在 `record_ids` 中；没有启动任何真实 agent。
   4. 该来源通过公开入口 save B，指定 `--supersedes A`；再运行 show 收取，核对操作已入库且取代关系存在。
   5. 断言 A 应被 B 取代，实际失败：`left: None`，`right: Some(B)`，程序退出 101。此前的收取成功与关系存在断言均通过，排除了来源未授权或暂存未收取导致的假复现。

   本轮观测值：restore 时间 `2026-10-05T07:34:56.800Z`，之后写入的 B 时间 `2026-10-05T07:34:54.847Z`；`replaced_by=null`，`default_list_has_A=true`、`injection_has_A=true`、`export_has_A=true`。这是合成倒序状态下的确定结果；未声称真实系统时钟曾回拨。

   **改法**：让取代、撤回、恢复共同遵守能表达动作先后的顺序，保证 restore 只撤销先于它的动作，后续新取代照常生效；不能仅给 retraction / restore 提高时间而留下 save / 收取仍使用另一套顺序。补上述“恢复后再次取代”的单个回归即可。若修复需要改 ingest / render 或设计，应由主控明确返工范围；不要改旧记录、加 sleep，或把已成功收取的新取代静默忽略。

### 本轮验证与范围

- 前台运行 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test -p cairn --test cmds --test render_session`：cmds 12、render/session 12，共 24 项通过，包含两项返工回归。
- 前台运行规定共享 target 下的 `cargo clippy --all-targets -- -D warnings`，通过；`git diff --check cd8ac7b..d844513` 通过。没有重复全量测试或扩展覆盖矩阵。
- 核对完整差异：只有 commands、cmds 测试、获批的 DESIGN 边界和返工记录；除上述动作顺序问题，本轮未发现其他新增问题。未重开已批准决定或前次六项取舍。
- 审查 worktree 保持干净，未修改代码、测试、分支或提交；只在主仓库本文件追加复核意见。一次性 Rust 程序和合成数据在临时目录运行后清理，不保留在 worktree 中。

## 第二轮复核

**可以合并**。

2026-10-05，复核 detached `d91440040d5838d7bee478528b7376ca85607955`；仅检查上一轮剩余问题、`git diff d844513..d914400` 及固定渲染期望是否改变。上一轮剩余必须改已修好，可以关闭。本轮必须改 0 条、建议改 0 条、可以不改 0 条；不重复计算前次实现取舍。

### 修复核实

- `crates/cairn/src/render.rs:67–74` 统一用 `x.rowid > n.rowid` / `x.rowid > t.rowid` 判断 restore 是否晚于取代 / 撤回；多个仍有效的取代也按取代记录 rowid 选最后入库的一条。因此旧 restore 不再抵消后来入库的新取代，与获批的 DESIGN §6.2 一致。
- `commands.rs` 已完整移除“最大动作时间 + 1 ms”及相关 import，恢复正常 `now()`；校验和追加仍在同一 IMMEDIATE 事务内。核对生产写入路径：commands 和 ingest 都在 IMMEDIATE 事务中插入 records，取代关系与其 checkpoint 同事务提交；delete 只更新墓碑字段，不删除行。没有发现 records 的替换插入、物理删行或执行 VACUUM 的路径。本轮未修改 schema 或这些写入机制。
- 独立重建一次性 Rust 程序，复跑原先的倒序时间及“恢复后再次取代”路径：临时库中将旧撤回时间设为当前时间加 2 秒，restore 成功，时间仍位于调用前后的当前墙钟之间，旧撤回记录不变；随后通过库级 SessionStarted 将 A 注入合成来源，再由该来源 save B 并指定 supersedes A。确认 B 已 ingested、取代关系存在后，A 的 `replaced_by` 正确指向 B。
- 在同一合成用例中再将 B 的时间置为 `2000-01-01T00:00:00.000Z`，A 仍被 B 取代，默认 list、无参数 show、export 和实际库级注入均不带出 A 的旧正文；新正文正常显示。再次 restore A 后，取代解除且 A 重新进入注入。程序全部断言通过、退出 0；观测到撤回、首次恢复、B、再次恢复的 rowid 依次为 `2 < 3 < 4 < 5`。仅修改合成库时间，未调整系统时钟、未启动真实 agent。

### 差异与验证

- 完整差异仅包含授权的 render 动作排序、commands 移除时间推进、cmds 回归、主控批准的 DESIGN 约束及返工记录；本轮范围内未发现新增问题。展示顺序与最新更正仍沿用原有时间排序，未混入本次动作顺序变更。
- **固定渲染样例期望未改**：`tests/render_session.rs` 在 `d844513` 与 `d914400` 的 Git blob 均为 `ba7b4ceb81490597ce0da5955acb43eb6889a6a5`，整个测试文件逐字相同；`fixed_example_visibility_corrections_events_and_local_changes` 的正文逐字断言实际通过。cmds 中被替换的是上一轮时间推进的实现断言，现按批准设计验证使用当前墙钟；实际恢复、后续撤回、旧记录不变、无可恢复状态不追加等断言仍保留。
- 前台运行 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --all-targets`，86 项全部通过：cmds 13、render/session 12、save 19、scope/facts 5、store 17、turn 16、probe 4。规定共享 target 下的 `cargo clippy --all-targets -- -D warnings` 通过；`git diff --check d844513..d914400` 通过。
- 只在主仓库本文件追加意见，未改审查 worktree 的代码、测试、分支或提交。临时程序显式使用隔离 HOME / XDG_STATE_HOME、数据库和暂存可信根；程序与合成数据验证后清理，不留在 worktree 中。
