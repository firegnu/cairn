# 任务：2e 回合判定（TurnStarted / TurnEnded / SessionEnded 核心函数）

2026-10-05，cairn/main（主控）交给 cairn/dev-turn（Codex，重：gpt-6-astra / xhigh）。
路由：重 / 交叉审查要 / 影响面：碰要害（路由：重、要、碰要害）
类型：功能变更
依据：本轮做 TurnStarted、TurnEnded、SessionEnded 的核心函数（入参是已解析好的值，不含 hook 的 JSON 解析和输出格式）；不做注入渲染（2d 并行在做）、用户命令（2f）、hook 入口与 install（阶段 3）。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩一节）
- `docs/DESIGN.md` §3（回合、确认、续跑）、§5（来源）、§6.2（`turn_decisions`、`confirmations`、`events`）、§6.5（收取）、**§8.3 全文**（含"回合起点的存法"）、§8.4、§8.6、**§9.3**、§11.1
- `docs/实施计划.md` 阶段 2 的 2e 一行
- 已合并的模块（直接用，不改已有行为）：`store.rs`、`spool.rs` / `ingest.rs`（收取，以及 2c 返工时加的"某来源窗口内是否还有未处理暂存文件"的有预算查询，结果分有 / 没有 / 未知）
- `docs/tasks/P2c-adopt与save.md` 的完成记录和返工记录（时间格式、接口约定）

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p2e-turn`，分支 `p2e-turn`（已从 main 建好）。
- 新建模块（例如 `turn.rs`），在 `lib.rs` 声明。测试放 `crates/cairn/tests/` 下以 `turn` 开头的文件。本轮不加 CLI 子命令。
- 并行任务：cairn/dev-render 在分支 `p2d-render` 上写渲染和 `cairn show`。你不要动它的文件；两边都会在 `lib.rs` 加一行 `mod`，冲突由主控合并时处理。
- 编译：所有 cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
- **TurnStarted**：项目已采用且未禁用时，登记 / 更新来源（`claude:<sid>` / `codex:<sid>`，`association='hook'`），写一条 `events`（`kind=turn_started`，`detail` 存 turn_key，`at` 是收到时间）。
- **TurnEnded**：照 §8.3 第 1–7 步实现，返回"放行"或"请续跑 + 原因"。要点：
  - 禁用 → 放行；未采用 → 放行，不收取、不记录；收取暂存区（当前来源优先）；
  - `continued` 为真：窗口内有已提交确认写 `confirmed`，否则写 `unconfirmed_after_continue`，放行，绝不再续跑；
  - 只认**已提交**的 `confirmations`；窗口按 §8.3 第 5 步和"回合起点的存法"（最近一条 `turn_started`，否则上一次 TurnEnded 判定，否则会话开始）；
  - 没找到确认：用 2c 的查询看这个来源窗口内有没有未处理暂存文件，"有"或"未知"都写 `pending_unprocessed` 并放行，不续跑；
  - 否则插入 `(source, turn_key, continue_requested)`：插入成功返回续跑，原因用 §9.3 原文（填来源 ID），窗口内有 `save_rejected` 事件时附上拒收原因（不含正文）；唯一键冲突说明是重复事件，放行；没有 turn_key 时不续跑、写 `skipped`（见 DESIGN §8.3 修订）。
- **SessionEnded**（§8.4）：写一条 `session_ended` 事件，不收取、不渲染，尽快返回。
- **失败策略**（§8.6）：对外提供的入口在任何错误时都返回"放行"，同时把错误交给调用方（阶段 3 负责写 `errors.log`）。
- 判定逻辑写成纯函数（输入是已查到的事实，输出是决定），数据库读写放在外面一层，方便测试。
- 时间一律用 2c 定的 RFC 3339 UTC 毫秒字符串；"当前时间"由入参传入，保证测试可重复。

## 怎么算做完
以下验收条件经用户 2026-10-05 同意，取自实施计划 2e 的"建议验证"：
1. 同一事件重复送达：同一个 TurnEnded 送两次，不会续跑两次。
2. 乱序到达：例如 TurnEnded 先于 TurnStarted、或两个 TurnEnded 交错，结果仍满足"每回合最多续跑一次"，不出错。
3. 续跑后再次 Stop（`continued` 为真）：不再续跑，正确写 `confirmed` 或 `unconfirmed_after_continue`。
4. 没有 turn_key：不续跑，写 `skipped` 并放行（DESIGN §8.3 已按 2e 发现的漏洞修订）；同一事件重复送达也不续跑。
5. hook 出错时放行：数据库忙、数据库出错等情况下入口返回"放行"并带回错误。
6. 每个回合最多续跑一次（贯穿以上各条）。

另外按任务要求：只认已提交确认（暂存区里没处理的文件不算确认，记 `pending_unprocessed` 不续跑）；有拒收时续跑原因附上拒收原因且不含正文；禁用 / 未采用时什么都不写。

验证只做这些：上面各条一个测试（纯函数部分可以用表驱动），再补你判断直接相关的边角用例（完成记录里列出）；`cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各跑一次并通过。测试只用临时 `XDG_STATE_HOME` / `HOME` / 临时 Git 仓库，暂存区可信根指向临时目录。觉得不够，在回复里说，不要自己加。

## 不要做
- 不要读写真实的 cairn 数据库、真实 `~/.local/state`、真实临时目录下的 `cairn-spool`；不要读真实会话记录、工具记忆。不用 Python。
- 不要改 store / schema / scope / facts / spool / ingest / save 的已有行为，不改表结构；需要它们加小接口时停下来报告。
- 不要做渲染、SessionStarted、用户命令、hook 的 JSON 解析与输出、`errors.log`、install。
- DESIGN 说不通的地方停下来报告，等主控决定。
- 不要按项目名或路径批量杀进程。
- 不合并到 main，不推送。只在 `p2e-turn` 分支上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 完成记录

2026-10-05，cairn/dev-turn（Codex），分支 `p2e-turn`。

**做了什么**：先按主控指示执行 `git merge main`，快进至 `9f6f20e`，带入缺失 turn_key 时不续跑的设计和任务书修订。新增 `turn.rs`，在 `lib.rs` 声明；提供纯函数 `decide(Facts) -> Outcome`，以及 `turn_started` / `turn_ended` / `session_ended` 三个入口。`Context` 接收数据库词法路径、暂存可信根、已解析的项目键、agent / session_id、收到时间和禁用标记；不读环境或调用 Git。入口返回 `Report { action, errors }`，错误时放行，错误交由阶段 3 记录。

TurnStarted 在事务中登记 hook 来源并记录起点；TurnEnded 先检查禁用和采用状态，复用 2c 的来源优先收取和三态 pending 查询，仅以已提交的 confirmations 判定确认。用已有唯一键的冲突忽略插入保证最多请求一次续跑；续跑原因保留 §9.3 原文，只从本来源窗口内的拒收事件提取 reason，不读或输出记录正文。SessionEnded 只写结束事件。未修改地基模块、收取接口、表结构或 CLI。

**验证了什么**：所有命令均在前台等待结束，所有 Cargo 命令使用指定共享编译目录。按行为逐步取得有效 RED→GREEN：开始入口尚无落库行为导致来源缺行；结束入口未请求续跑；纯判定缺少优先级；continued 再次请求续跑；未收取有效暂存文件；把积压文件误判为可续跑；续跑原因漏掉拒收原因；会话结束未写事件；pending Unknown 未向调用方报告错误。最初测试夹具的生命周期编译错误不计作 RED。

`crates/cairn/tests/turn.rs` 共 15 项测试，覆盖任务六条验收及明确要求：重复 Stop、结束先于开始和两个回合交错、continued 的 confirmed / unconfirmed_after_continue、缺 ID 重送、数据库忙 / SQL 失败放行、四个并发入口对同一回合只请求一次；暂存收取后确认、未处理 / 未知暂存记 pending、拒收原因和正文隔离、禁用 / 未采用无操作。

直接相关边角用例：
- 两种 agent 来源登记及更新、事件时间和 turn_key 持久化；SessionEnded 不登记来源、不收取，保留暂存文件。
- 续跑前后回合 ID 改变仍不再续跑；无 ID 时有 / 无 TurnStarted 两种降级路径，窗口推进也只写 skipped。
- 最近 TurnStarted、上次判定、会话开始三个窗口层级和来源 first_seen 兜底；包含窗口起点，排除窗口之前和其他来源的确认；同时覆盖 saved 和 nothing_new。
- 未提交的确认不算确认，锁释放并回滚后仍能请求续跑；失败的 TurnStarted 完整回滚来源更新。
- 收取事务失败时仍可写 pending / confirmed，但不会请求续跑或占用请求键；撤销合成故障后文件能收取，尚未请求过的回合仍有续跑机会。
- 数据库尚不存在时不建库、不建暂存区；拒收原因按来源和窗口过滤。

最终 `cargo test --all-targets` 一次通过：turn 15、save 19、scope/facts 5、store 17、probe 4，共 60 项；`cargo clippy --all-targets -- -D warnings` 一次通过。`cargo fmt --all -- --check` 和 `git diff --check` 通过。新增测试显式传入临时 HOME / XDG_STATE_HOME 路径值、临时非 Git 项目和暂存可信根，不改变进程环境；现有全量测试继续使用自身隔离夹具。未访问真实数据库、暂存区、配置或会话。

**拿主意的地方**：严格保持 §8.3 顺序，continued / 已确认 / pending 优先；走到请求续跑这一步却没有有效 turn_key 时，键用 `no-key:<窗口起点>` 写 skipped。没有可观测的回合或会话起点时不编造时间，带错误放行。会话起点先取最早的 session_started，缺失时取来源 first_seen。确认和暂存查询均使用包含起点的下界，与 2c 接口一致。

pending 查询固定使用 100 ms 预算，不改变 2c 收取的 50 文件 / 300 ms 预算。查询后用短 IMMEDIATE 事务重查采用状态、窗口和确认；窗口被并发 hook 改变时，旧暂存查询不能证明新窗口没有文件，按 Unknown 放行。2c 的 Unknown 不提供底层原因，因此返回固定 `PendingUnknown` 错误供阶段 3 日志记录，不猜测根因。收取报错不阻止后续观测，但任何错误都禁止返回续跑、也不消耗续跑请求键。本模块新增的数据库时间使用传入的收到时间，2c 收取自身的时间约定保持不变。暂存查询仍是有预算的观测，不声称与并发发布形成跨文件系统 / 数据库原子快照。

**没做的事**：未做渲染、SessionStarted、hook JSON 适配及输出、errors.log、用户命令或 install；未改变 DESIGN（只合入主控修订）、HANDOFF、真实配置及其他模块行为，未开或委派 agent。未合并回 main、未推送。没有新增需主控决定的事项；交叉审查与合并留给主控。
