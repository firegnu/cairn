# 交叉审查：2c adopt / save 与暂存区收取

2026-10-05，cairn/main（主控）交给 cairn/dev-review-save（Codex，重：gpt-6-astra / xhigh）。
类型：调研
提示：区分已查证事实、推测和未知，给出直接依据；结论限定在本轮调查范围。
你是被委派的审查者：只读审查，不要开别的 agent。**本文件在主仓库工作区（`/Users/firegnu/Developer/personal_projs/cairn/docs/tasks/`），不在你的审查 worktree 里；你只写本文件这一个文件。**

## 背景
- cairn/dev-save（Codex）在分支 `p2c-save` 上实现了 `adopt` / `unadopt` / `save`、暂存区（`spool.rs`）和收取（`ingest.rs`），提交 `f3b9abc`。任务书 `docs/tasks/P2c-adopt与save.md`（审查 worktree 里也有，末尾有完成记录）。
- 主控已审过：没动地基文件（store / schema / scope / facts）和 DESIGN；重跑 `cargo test --all-targets`（41 项，新增 15）、clippy、`git diff --check` 通过；真实临时目录下没有 `cairn-spool`。

## 先读
- `AGENTS.md`（规矩一节）
- 任务书与完成记录：`docs/tasks/P2c-adopt与save.md`
- `docs/DESIGN.md` §5、§6.2、**§6.5**、§8.2、§12
- 了解地基约定：`docs/tasks/P2a-存储-交叉审查.md` 的审查意见

## 要审查的
- 审查 worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-p2c-save`（detached，指向 `f3b9abc`）。改动范围：`git diff main...HEAD`。**只读：不改、不提交、不切分支。**
- 可以跑：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test -p cairn --test save`、clippy；需要核实具体怀疑时，可以在临时目录里写一次性的 Rust 程序复现，不要留在 worktree 里。测试时可信根一律指向临时目录，**不要在真实的 `DARWIN_USER_TEMP_DIR` 下建 `cairn-spool`**。
- 不要运行 claude、codex，不要开 agent；不要读真实的 `~/.local/state`、真实会话记录、工具记忆、Corral 事件文件。不要用 Python。
- 验证预算：只针对下面的重点做定点核实；不录屏，不做覆盖矩阵。

## 重点看
1. **收取的幂等与事务边界**（§6.5 收取 1–6 步）：并发两个收取者、提交后删文件前退出、忙时回滚，是否都不会重复写确认 / 拒收 / 记录，或者丢掉一个本该收进来的操作。
2. **文件系统安全**（§6.5 文件安全、写入）：可信根条件、逐层 `O_NOFOLLOW` 打开、`fstat` 核验、`RENAME_EXCL` 发布、条目类型与属主检查、命名空间隔离；有没有检查后使用的空档，或者能让收取读 / 删暂存区以外的文件。
3. **隐私**：拒收事件、错误信息、日志里会不会带出正文；目标路径不匹配时是否真的不读正文。
4. **校验规则是否算对**（§8.2 第 5–8 步）：来源解析、采用状态、取代对象的三个条件、`--nothing-new` 与取代的关系、确认时间。
5. **对后续模块的接口**：`ingest` / `Spool` 的接口和时间格式，是否够 2d（注入）、2e（回合判定，需要按来源和时间窗查确认、查"未处理的暂存文件"）、阶段 3 的 hook 直接用；有没有会让它们出错的约定缺失。

## 输出
追加到本文件末尾「## 审查意见」：
- 先写一句结论：**可以合并** / **改完再合并**。
- 每条意见写：级别（必须改 / 建议改 / 可以不改）、位置（文件和行号）、问题、改法。
- **"必须改"只给两种情况**：任务书"怎么算做完"没达到；或者碰到的要害上有真实缺陷（会丢数据、并发出错、隐私正文泄露、越权读删文件、核心规则算错）。其余一律"建议改"。能给出复现步骤或直接依据的写上。
- 最后对完成记录里"拿主意的地方"逐条表态：同意 / 不同意及理由。

## 回复
回复里只写结论和各级别条数。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 审查意见

**改完再合并**。必须改 1 条，建议改 1 条，可以不改 4 条。

2026-10-05，cairn/dev-review-save。审查对象为 detached HEAD `f3b9abcfa063e6939f4fc65832114797a06d7649`，范围 `git diff main...HEAD`。以下结论限于任务指定的事务、文件安全、隐私、校验和后续接口；未修改实现、正式测试、设计、分支或提交。

1. **必须改：来源优先的全量预扫描会耗尽全部预算，使合法暂存操作反复零进展。**

   位置：`crates/cairn/src/ingest.rs:36–55`、`:61–70`；`crates/cairn/src/spool.rs:194–228`。

   问题：传入 `preferred_source` 时，先逐个打开并读取所有候选的头部，完成排序后才进入第一个收取事务。扫描和收取共用 300 ms 截止时间；扫描一旦把预算用完，事务循环立即退出。下一次又从头扫描同一批文件，因此“剩下的留给下一次”不能保证向前推进。SessionStarted / TurnEnded 按设计都会传当前来源，直接受到影响。

   已查证的复现：在临时可信根和临时状态库中登记一个已采用项目、一个已知来源，通过本版 `Spool::publish` 发布 4,000 个规范 ULID、相同已知来源、合法时间的 `nothing_new` 操作，没有损坏文件或数据库锁。连续三次调用 `ingest(..., Some(source))`，每次约 300 ms，均返回 `processed=0, ingested=0, rejected=0, replayed=0`，待收取数一直为 4,000。随后对同一批文件调用 `ingest(..., None)`，立即收进 50 个，剩余 3,950 个，证明输入可正常入库。

   这不是已观测到物理删除文件，而是已复现正常 hook 收取路径无法排空合法积压的核心功能缺陷；若持续只靠该路径，保存记录始终不能用于接续，且会一直暴露于设计 R9 的临时文件清理风险。4,000 是本次复现规模，不声称它是所有机器的触发阈值。

   改法：避免把全部优先级预扫描作为第一笔事务的前置条件；在预算内尽早处理已识别的优先操作，并为事务处理保留预算，使重复调用能够继续推进，而不是重复耗尽在同一轮扫描上。保留来源优先、ULID 顺序、50 个 / 300 ms 和目标不匹配不读正文的边界，不能靠加长超时解决。补一个针对积压下来源优先收取仍能前进的回归。

2. **建议改：在 2e 接入前，明确按来源和确认窗口判断“未处理暂存文件”的接口。**

   位置：`crates/cairn/src/spool.rs:64–69`、`:101–115`、`:196–240`；`crates/cairn/src/save.rs:38–58`；`crates/cairn/src/ingest.rs:19–25`。

   已查证：公开的 `Status` 只有整个命名空间的 `.json` / `.tmp` 数量；它不核对目标路径、来源、时间和 `spool_ops`，`Report` 也没有剩余操作信息。来源在 header，`created_at` 在 payload。现有 crate 内部的 `names` / `candidate` / `payload` 足以继续实现检查，但后续调用者不能直接把 `pending_json > 0` 当成 §8.3 的 `pending_unprocessed`，也不能把全部残留文件当成当前来源本窗口的操作。

   改法：由 2c 与 2e 接口约定一个有预算的查询或封装，核对本数据库目标、声明来源和 `created_at` 窗口，并结合 `spool_ops` 排除已提交但尚未删除的重放文件；读取出错或预算不足时保留“未知”，交由 hook 按错误放行策略处理。继续复用句柄相对访问，不让 2e 用 `Spool::path()` 自行重新打开路径。当前任务只要求供 status 使用的数量函数，尚无 2e 错误实现，因此本条不作为合并门槛，也不要求改表结构。

3. **可以不改：每个操作的事务、重放与竞争处理在本轮验证中成立。**

   位置：`crates/cairn/src/ingest.rs:72–93`、`:102–108`、`:129–202`；`crates/cairn/src/spool.rs:243–251`；`crates/cairn/tests/save.rs:264`、`:446`。

   已查证：`BEGIN IMMEDIATE` 之后先查 `spool_ops`，采用与取代校验、记录 / 关系 / 确认或拒收事件、`spool_ops` 都在同一事务中。提交成功后才 unlink，另一收取者已删文件的 ENOENT 被视为成功。既有测试复跑通过：正文、无新内容、拒收的提交后重放均不再写库；锁冲突和确认插入故障保留原文件且回滚，busy timeout 恢复为调用者原值。

   临时程序另让两个独立 Store / Spool 收取者同时竞争三种操作，先用第三连接持有 IMMEDIATE 锁，再释放：一个报告处理 3 个（接受 2、拒收 1），另一个报告重放 1 个；最终 `records=1, confirmations=2, events=1, spool_ops=3`，无残留 `.json`。本次确实走到两个收取者持有同一操作并发生重放的分支，没有重复确认、拒收或记录。

   改法：保留事务与提交后删除顺序。删除失败发生在提交之后，调用者仍应允许之后按 `spool_ops` 重放清理；不要把错误返回等同于此前没有提交。这里没有要求扩展为多文件大事务。

4. **可以不改：文件安全和命名空间隔离符合本轮约定，未发现可复现的越界读删。**

   位置：`crates/cairn/src/spool.rs:35–55`、`:74–91`、`:118–191`、`:243–297`、`:340–376`。

   直接依据：可信根打开后检查句柄类型、属主和组 / 其他人不可写；`cairn-spool` 与命名空间逐层 `openat(O_DIRECTORY | O_NOFOLLOW)`，然后 fstat 检查私有权限。后续枚举、文件打开、发布、删除均相对保留的目录句柄。条目以 `O_NOFOLLOW | O_NONBLOCK` 打开后核对普通文件和属主；读到的内容来自被核验的同一文件句柄。发布独占创建 0600 临时文件、fsync，再用 `RENAME_EXCL`，没有普通 rename 回退。

   既有定点测试复跑通过：两层目录链接、文件链接、目录 / FIFO / socket 条目不会被收取，链接目标未改变；宽权限目录拒绝；已有 final / tmp 不被覆盖；不同数据库的命名空间互不处理。根路径本身不强加 `O_NOFOLLOW` 与 DESIGN 的“打开可信根后检查句柄”一致，不能套用子目录规则误判。

   改法：保留现有访问方式。本结论覆盖本轮代码审查和这些合成用例，不宣称穷尽同 uid 进程任意改写、移动文件的所有竞争，也不把普通 unlink 说成磁盘安全擦除。

5. **可以不改：目标检查与拒收路径没有发现正文泄露。**

   位置：`crates/cairn/src/spool.rs:194–240`；`crates/cairn/src/ingest.rs:68`、`:117–158`、`:196–202`；`crates/cairn/src/cli.rs:30–40`。

   直接依据：v1 头部用容量为 1 的读取器读到换行，在目标路径比较成功前不读 payload；目标不匹配返回 None，既不拒收也不删文件。头部解析错误被丢弃，payload 解析错误映射为固定原因，`events.detail` 仅写操作 ID 和固定原因，`spool_ops.reason` 同样不带正文。

   既有测试复跑通过：目标不匹配并带损坏 payload 的文件原字节保留，没有拒收事件；ID、版本、时间、payload 错误的合成私密正文不出现在拒收事件中。CLI 的正文错误使用固定说明。当前没有新增日志写入实现，阶段 3 应延续该错误边界。

   改法：保留目标检查和固定错误原因；修第 1 条时也不能为了减少读调用而让头部缓冲预读其他目标的正文。无法辨认目标的损坏头部保留文件是可接受的保守处理。

6. **可以不改：来源、采用、取代规则和已提交确认的时间语义正确。**

   位置：`crates/cairn/src/ingest.rs:110–150`、`:161–227`、`:231–253`；`crates/cairn/src/save.rs:23–33`、`:60–90`；`crates/cairn/src/adopt.rs:7–23`；`crates/cairn/src/cli.rs:21–26`、`:119–125`。

   已查证：缺失 / 未知声明来源按操作 ID 分配 `local:<op_id>`，关联为 uncertain；已知来源保持原身份。采用校验和每个取代对象的存在且未删除、同项目且同工作线、曾注入当前解析后来源三个条件在写入前完成，任何一个失败整条拒收。重复合法取代 ID 只写一条关系。CLI 禁止 `--nothing-new` 搭配取代，收取层也检查该组合。

   确认 `at` 与记录 `created_at` 使用暂存时间，`spool_ops.processed_at` 使用处理时间；新时间值统一为 RFC 3339 UTC 毫秒、Z 结尾。`StoredFacts` 保留事实含义并转换采集时间，后续读取 facts 应使用这个持久化类型。2d 可读取记录、事实和取代关系；2e 的已提交确认可按 `source_id` 与文本时间窗口查询，已有索引支持；未处理文件的接口注意第 2 条。adopt / unadopt 先收取再改变状态，重复命令不刷新变更时间。

   改法：保留这些规则与时间格式。除第 1 条外，本轮未发现五条“怎么算做完”及四组补充验收的实现缺口；既有 15 项 save 测试全部通过。

验证记录：所有命令均前台等待结束，Cargo 均使用 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。本轮完成 `cargo test -p cairn --test save`（15/15）、`cargo clippy --all-targets -- -D warnings`、`git diff --check main...HEAD`，均通过。没有重复跑主控已完成的全量测试。

额外验证只做上述双收取者竞争与来源优先积压两项。一次性 Rust 程序位于 `/private/tmp/cairn-p2c-review.aRbwJx/`，可用以下命令分别复跑：

```sh
CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo run --offline --manifest-path /private/tmp/cairn-p2c-review.aRbwJx/Cargo.toml -- concurrent
CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo run --offline --manifest-path /private/tmp/cairn-p2c-review.aRbwJx/Cargo.toml -- backlog
```

程序只调用显式临时路径的 Store / Spool，不调用真实 agent 或读取 HOME 下的数据；临时数据库和暂存区随 TempDir 退出回收。没有在真实 `DARWIN_USER_TEMP_DIR` 下建立 `cairn-spool`。审查 worktree 保持干净，仓库内只向本文件追加意见。

对完成记录“拿主意的地方”逐条表态（决策复核，不另计意见条数）：

- **同意** v1 是完整 JSON 对象、首行 header / 后续 payload 的格式；它明确了目标检查边界。**同意**头部读取不预读正文；**不同意**把所有头部扫描完成作为开始处理的条件，第 1 条必须修正。
- **同意**损坏头部无法可靠辨明目标时保留文件，不冒险拒收或删除其他目标的操作。
- **同意**跳过非规范 ULID 文件名；程序生成的名称均为规范 ULID，异常文件由 status 暴露残留。
- **同意**文件名与内容 ID 不同时，以文件名 ID 记录幂等拒收，避免内容中的 ID 占用其他操作；拒收仍须在同一事务里完成。
- **同意**使用固定拒收原因，不将解析错误中的正文带入事件或错误输出。
- **同意**由 `StoredFacts` 在持久化边界转换 2b 的 Unix 毫秒，其余字段保持原义，以及所有新增时间采用统一的 UTC 毫秒字符串；后续模块不要再把持久化 JSON 反序列化成原始 `GitFacts`。
- **同意**重复 adopt / unadopt 不刷新时间、未登记项目的 unadopt 为无操作；重新 adopt 更新采用时间并保留上次停用时间，当前状态仍由 adopted 字段表达。
- **同意** `--nothing-new` 与取代互斥，因为没有新记录承载关系；同意重复合法目标只写一次关系。
- **同意**库级调用者用同一次 `database_path` 计算结果打开 Store 与 Spool 的前提；当前 CLI 满足这一点，阶段 3 应沿用，不能把词法路径与自行 canonicalize 的路径混用。
- **同意**不承诺强制中断系统调用的协作式截止边界；这不能豁免第 1 条已证实的反复零进展。来源优先和有界收取的目标应保留，应调整扫描与处理的衔接，不建议扩大时间预算。

## 复核意见

**改完再合并**。必须改 1 条，建议改 0 条，可以不改 2 条；上一轮第 1 条部分修复，仍未关闭，第 2 条建议已落实。

2026-10-05，cairn/dev-review-save。复核对象为 detached HEAD `64efe79b2660632b0e1fd232df37a56daba30cf2`，仅检查上一轮第 1、2 条及 `git diff f3b9abc..64efe79` 的新增影响。没有重新展开原有设计或把无关旧问题升级为合并门槛。

1. **必须改：第 1 条尚未修完；其他来源的积压仍能让当前来源及第二遍收取反复零进展。**

   位置：`crates/cairn/src/ingest.rs:149–167`、`:168–180`；`crates/cairn/tests/save.rs:264`。

   已修好的部分：第一遍遇到优先来源就立即收取，上一轮“4,000 个文件全部属于当前优先来源”的样例现在连续三轮各处理 50 个，耗时分别为 16 / 15 / 16 ms，剩余数为 3,950 / 3,900 / 3,850。新增正式回归也验证了这一种排列，复跑通过。

   未修好的部分：第一遍对非优先来源只读头部后跳过，只有整个第一遍结束才处理其他来源。因此旧来源文件足够多时，仍会先耗尽 300 ms；下一次仍从同一批旧文件开始，既到不了第二遍，也可能到不了排序靠后的当前来源文件。这仍是上一轮指出的“来源优先扫描耗尽预算、重复调用无法前进”，不是另找的无关旧问题。

   已查证的定点复现：沿用上一轮临时程序的 Fixture、合法 Operation 和 `Spool::publish`，同一组数据先完成上述三轮成功收取，然后登记第二个已知来源 B：

   - 剩余 3,850 个文件全部属于来源 A，调用 `ingest(..., Some(B))` 三次，每次约 300 ms，`processed / ingested / rejected / replayed` 全为 0，待收取数一直为 3,850。
   - 再发布一条来源 B 的合法操作，其 ULID 排在 A 的积压之后；每次重新打开 Spool，继续调用三次，仍各约 300 ms、处理数全为 0，待收取数一直为 3,851，B 的确认数为 0，B 文件仍存在。
   - 对同一批文件调用 `ingest(..., None)`，正常收进 50 个，剩余 3,801 个。复现没有损坏文件、锁冲突或非法来源。

   这对应旧会话有积压、开启新会话后按新来源优先收取的实际路径。观测到的是文件保留但始终不入库，没有将它夸大为已发生物理删除。此次修复不能只保证“积压全部等于当前来源”的情况。

   改法：继续调整扫描与事务处理的衔接，保证优先来源不存在或排在大量其他来源之后时，多次调用仍能推进；不能把完整扫过所有非优先头部作为任何事务开始的必要条件。保留 50 个 / 300 ms、来源优先及目标不匹配不读正文的要求，不靠扩大预算或取消隐私检查解决。针对上述同一缺陷补一项跨来源积压回归；若具体方案需要调整设计语义，应先报告，而不是在实现中悄悄放宽。

2. **可以不改：第 2 条建议已实现，新增 pending 查询在本轮范围内未发现错误。**

   位置：`crates/cairn/src/ingest.rs:27–131`；`crates/cairn/src/spool.rs:71–75`、`:124–174`、`:204–246`；`crates/cairn/tests/save.rs:305`、`:404`。

   已查证：接口明确按声明来源、包含起点的 `created_at >= since` 窗口查询，先排除 `spool_ops` 已提交操作，再用现有句柄访问核验数据库目标、头部及时间。它不把暂存文件当作已提交确认，也没有调用保存校验去提前拒收文件。已知匹配返回 Yes；完整扫描没有匹配才返回 No；非法窗口、无法解析的相关数据、读取 / SQL 错误和预算耗尽返回 Unknown。查询结束恢复原 busy timeout，不改数据库行、不删文件。

   两项新增测试复跑通过，覆盖来源与窗口起点、不同目标且损坏正文、已提交未删除、tmp / 链接跳过、目录路径替换后仍使用原句柄，以及损坏输入、读权限 / SQL 错误和预算耗尽。`Candidate::Skipped` 与 `Unknown` 的区分，以及枚举超时显式报错，避免把不完整观察报告成 No；目标不匹配仍在读取 payload 之前退出。

   改法：保留接口及三态语义。同意返工记录明确它不提供文件系统与数据库的原子快照；2e 接入时必须按文档将 Unknown 走错误放行路径，不能当成 No，也不能把 Yes 当成已确认。本轮没有发现需要为此继续修改 2c 的问题。

3. **可以不改：除此之外，本次差异没有发现新增的事务、隐私或文件安全回归。**

   位置：`crates/cairn/src/ingest.rs:142–194`；`crates/cairn/src/spool.rs:146–154`、`:204–234`；`crates/cairn/tests/save.rs:230`、`:489`、`:671`。

   直接依据：两遍循环共用原来的处理数与截止时间，未增加超时；正常小批量下仍先处理当前来源、再处理其他来源，每遍保留文件名顺序。`collect_one`、IMMEDIATE 事务中的去重 / 校验 / 写入、提交后 unlink 顺序均未改动。候选枚举和读取仍相对保留的目录句柄，头部读取容量仍为 1。目录枚举超时返回错误发生在入库前，不会把半份目录列表冒充完整列表。

   已查证：18 项 save 测试全部通过，包括原来的来源优先、三种重放、忙时回滚、符号链接 / 命名空间和目标正文隔离；clippy 与提交差异的空白检查通过。新增测试没有删除或放宽原有断言。第 1 条是仍未解决的收取进展问题，不以这些通过结果替它背书。

   改法：保留上述边界和回归。本轮没有另行发现需记录的无关旧问题，因此建议改为 0 条。

验证记录：前台完成 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test -p cairn --test save`（18/18，约 24 s）、`cargo clippy --all-targets -- -D warnings`（同一编译目录）及 `git diff --check f3b9abc..64efe79`，均通过。没有扩大为全量重审，也没有重复开发方已完成的全量测试。

定点探针在上一轮临时工程新增 `src/bin/recheck.rs`，复用原程序的夹具，原 `src/main.rs` 未改。它先验证同来源已修好，再用同一组积压验证来源切换后的停滞；每次带来源优先的收取均重新打开 Spool。复跑命令：

```sh
CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo run --offline --manifest-path /private/tmp/cairn-p2c-review.aRbwJx/Cargo.toml --bin recheck
```

探针正常退出，末行 `same_source_fixed=PASS mixed_source_stall=REPRODUCED new_source_confirmations=0`。所有数据均为显式临时目录下的合成数据，TempDir 退出后已回收；没有在真实可信根建立 `cairn-spool`，没有运行真实 agent 或读取真实状态、会话和工具记忆。只在本文件追加复核意见，审查 worktree 的实现、测试、分支与提交均未修改。

## 第二轮复核

**可以合并**。必须改 0 条，建议改 0 条，可以不改 3 条；上一轮复核第 1 条已关闭。

2026-10-05，cairn/dev-review-save。复核对象为 detached HEAD `35306cdd9e5c8a47d3f3a760a16b6650964aca4f`，范围仅为上一轮收取停滞问题、`git diff 64efe79..35306cd` 的新增影响，以及与主控设计提交 `9585ea1` 的一致性。

1. **可以不改：跨来源积压现在能持续推进，上一轮第 1 条关闭。**

   位置：`crates/cairn/src/ingest.rs:144–177`、`:183–215`；`crates/cairn/tests/save.rs:305`。

   已查证：来源发现只使用前 100 ms，扫描到的候选按优先来源分组并保留 ULID 顺序，再接上尚未扫描的名称。处理不再以完整扫过所有非优先头部为前提。没有当前来源候选时，已找到的其他来源文件可以使用剩余预算提交；靠后的当前来源文件也能随着积压减少而被发现。

   复用上一轮临时程序的同一夹具和排列，观察结果如下：

   - 4,000 个来源 A 的合法操作，以 A 为优先来源调用三次，每次收进 50 个，剩余 3,850 个。
   - 改为尚无文件的已知来源 B，连续三次仍各收进 50 个，耗时约 111–113 ms，剩余数依次为 3,800 / 3,750 / 3,700；上一轮此处三次均为 0。
   - 再发布一条 ULID 排在所有 A 文件之后的 B 操作，继续以 B 为优先来源。每次重新打开 Spool，每轮均处理 1–50 个；第 53 轮收进 B，第 75 轮清空全部积压。最终确认和 `spool_ops` 均为 4,001 条，B 确认恰好 1 条、文件已删除，A 的 4,000 条确认严格保持 ULID 顺序。

   新增正式回归也独立验证“B 无文件”和“B 在积压末尾”，没有放宽为允许零进展。改法：保留本次实现和该回归；本结论关闭的是已复现的跨来源停滞，不扩张为任意文件系统延迟下的硬实时保证。

2. **可以不改：实现与主控批准的 `9585ea1` 设计文字一致。**

   位置：`docs/DESIGN.md:237–239`、`:317`；`crates/cairn/src/ingest.rs:133–177`、`:183–204`。

   已查证：`9585ea1` 是本次 HEAD 的祖先，`git diff 9585ea1..35306cd -- docs/DESIGN.md` 为空，合入的设计没有被开发方另行改写。收取仍共用 50 个 / 300 ms 上限，前 100 ms 的扫描划分为事务预留时间；先处理扫描预算内找到的当前来源，再处理其他候选，未发现的当前来源允许延后。这符合修订后的“来源优先尽力而为”，不再按旧版严格优先的文字要求返工。

   候选按 `(是否非优先, 文件名下标)` 排序，各组保持 ULID 顺序；尚未扫描部分也沿名称顺序处理。探针确认靠后的 B 能在 A 全部清空之前入库，同时 A 的组内顺序保持正确。改法：保留该取舍。§8.3 的 `pending_unprocessed` 仍由后续回合判定模块执行，本轮没有据此声称已经完成 hook 或 2e 集成。

3. **可以不改：本次调度改动未发现新增的事务、文件安全或隐私问题。**

   位置：`crates/cairn/src/ingest.rs:158–176`、`:183–222`；`crates/cairn/tests/save.rs:230`、`:390`、`:489`、`:574`、`:756`。

   直接依据：扫描只保留候选下标，文件句柄随候选检查结束释放；正式处理重新走原有句柄相对访问和目标核验入口。扫描在某个头部耗尽预算时保留 `next` 位置，处理阶段仍可重试，没有把半读文件永久排除。已入候选的下标小于 `next`，与后续名称区间不重叠，没有引入同一次调用的重复排队。

   单文件 IMMEDIATE 事务、`collect_one` 的去重和业务校验、提交后 unlink、busy timeout 恢复均保留。`spool.rs`、三态 pending 查询、时间格式与地基模块未改；目标路径不匹配时不读正文的入口仍被复用。正式测试只新增一项，没有删除或放宽原有断言。

   已查证：19 项 save 测试全部通过，包含小批量来源优先、同来源与跨来源积压、pending 三态、忙时回滚、三种重放、链接与命名空间隔离；clippy 和提交差异空白检查通过。改法：保留现状。本轮限定差异内没有新增问题，也没有另行记录无关旧问题。

验证记录：所有命令均在前台等待结束，Cargo 均使用 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。完成 `cargo test -p cairn --test save`（19/19，约 53 s）、`cargo clippy --all-targets -- -D warnings`、`git diff --check 64efe79..35306cd`，均通过；未重复开发方已完成的全量测试。

临时工程新增 `src/bin/recheck2.rs`，保留上一轮探针，复用原 Fixture 和 Operation，把原来的停滞观测改为检查持续进展并收完同一批数据。复跑命令：

```sh
CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo run --offline --manifest-path /private/tmp/cairn-p2c-review.aRbwJx/Cargo.toml --bin recheck2
```

程序正常退出，末行 `mixed_source_progress=PASS total=4001 late_source_first_round=53 drain_rounds=75 pending=0 source_a_order=PASS`。轮数是本次机器上的观测，不是接口保证的固定值。数据库和暂存区均为显式临时路径中的合成材料，运行结束已随 TempDir 回收；未访问真实状态库、会话、工具记忆或真实 agent，未在真实可信根下创建 `cairn-spool`。审查 worktree 保持干净，仓库内只在本文件追加本节。
