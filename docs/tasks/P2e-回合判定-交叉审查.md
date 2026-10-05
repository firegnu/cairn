# 交叉审查：2e-回合判定

2026-10-05，cairn/main（主控）交给 cairn/dev-review-turn（Codex，重：gpt-6-astra / xhigh）。
类型：调研
提示：区分已查证事实、推测和未知，给出直接依据；结论限定在本轮调查范围。
你是被委派的审查者：只读审查，不要开别的 agent。**本文件在主仓库工作区（`/Users/firegnu/Developer/personal_projs/cairn/docs/tasks/`），不在你的审查 worktree 里；你只写本文件这一个文件。**

## 背景
- cairn/dev-turn（Codex）在分支 `p2e-turn` 上完成任务，提交 `9295e71`。任务书 `docs/tasks/P2e-回合判定.md`（审查 worktree 里也有，末尾有完成记录）。
- 主控已审过：没动地基模块和 DESIGN；重跑 `cargo test --all-targets`、clippy、`git diff --check` 通过。
- 另一个并行任务（2d / 2e）在别的分支，不在本次审查范围。

## 先读
- `AGENTS.md`（规矩一节）；任务书与完成记录；任务书"先读"里列的 DESIGN 章节。

## 要审查的
- 审查 worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-p2e-turn`（detached，指向 `9295e71`）。改动范围：`git diff main...HEAD`。**只读：不改、不提交、不切分支。**
- 可以跑该分支的测试和 clippy（命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`）；核实具体怀疑时可在临时目录写一次性 Rust 程序，不留在 worktree 里；暂存区可信根一律指向临时目录。
- 不要运行 claude、codex，不要开 agent；不要读真实 `~/.local/state`、真实会话记录、工具记忆、Corral 事件文件。不用 Python。只做定点核实，不做覆盖矩阵。

## 重点看
1. §8.3 第 1–7 步顺序与语义：禁用 / 未采用不写库；只认已提交确认；continued 时绝不再续跑；pending_unprocessed 不续跑；没有 turn_key 写 skipped 不续跑（9f6f20e 修订）。

## 输出
追加到本文件末尾「## 审查意见」：先写一句结论（**可以合并** / **改完再合并**）；每条意见写级别（必须改 / 建议改 / 可以不改）、位置、问题、改法。**"必须改"只给两种情况**：任务书"怎么算做完"没达到；或者碰到的要害上有真实缺陷（会丢数据、并发出错、隐私泄露、核心规则算错）。其余一律"建议改"。能给复现步骤的写上。最后对完成记录里"拿主意的地方"逐条表态。

## 回复
回复里只写结论和各级别条数。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 审查意见

**可以合并**。本轮发现必须改 0 条、建议改 1 条、可以不改 0 条；下文对实现取舍的逐项评价不另计问题条数。

2026-10-05，独立审查者 cairn/dev-review-turn。审查对象为 detached HEAD `9295e71d2d6af75af2499cddcb05f138cb41cda0`，范围为 `git diff main...HEAD` 的四个文件；未改代码、测试或 DESIGN，未提交、切分支或启动其他 agent。

### 已查证的行为与验证

- 禁用在打开数据库之前返回；未采用或数据库不存在时，均在打开暂存区及写业务表之前返回。TurnStarted 的来源登记和事件写入位于同一事务；SessionEnded 只写事件，不收取。
- `decide` 的顺序符合 §8.3：已提交确认优先；continued 且没有确认时记 `unconfirmed_after_continue`；随后才处理 pending、有效 turn_key 和 skipped。虽然代码先判断 confirmed，但 continued 的有确认/无确认两种情况都不会走到续跑分支，语义与第 4 步相同。缺少 turn_key 的 skipped 仅在前面的 confirmed / continued / pending 均未命中时适用，符合设计的顺序。
- 确认查询只读数据库中的 confirmations；暂存文件先经 2c 收取事务提交才能成为确认。待处理查询的 Yes / Unknown 都不能触发续跑。写判定前使用 IMMEDIATE 事务重查采用、窗口与确认，续跑请求通过唯一键冲突忽略插入，并且只有提交成功且确实新插入时才返回 Continue。
- 窗口按来源最近 turn_started → 上次判定 → 会话起点降级。重复与交错事件的去重不依赖窗口保持不变。续跑原因与 §9.3 原文一致；拒收只提取本来源窗口内事件的 reason，未读取 records 正文。
- 本轮前台运行 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --all-targets`：60 项全部通过（turn 15、save 19、scope/facts 5、store 17、probe 4）；随后运行同一共享编译目录的 `cargo clippy --all-targets -- -D warnings`，通过；`git diff --check main...HEAD` 通过。任务六条验收在既有测试中均有对应检查，包括四个并发入口仅一个返回续跑、未提交确认不生效及出错后不消耗请求键。
- 额外只做下面这一处定点核实，使用仓库外一次性 Rust 程序、临时数据库/项目/暂存可信根，程序及临时数据已清理。未读取真实数据库、暂存区、会话、配置、工具记忆或 Corral 事件。没有运行真实 agent，也不把合成测试通过推定为阶段 3 的 hook 适配已验证。

### 1. 建议改：暂存区打开失败也可保留后续可确定的判定

**位置**：`crates/cairn/src/turn.rs:158–161`，对照后面的 `179–221` 行。

**问题（已复现）**：`ingest` 报错会留在 errors 中并继续判定，但 `Spool::open(...)?` 报错直接结束整个入口。因此暂存目录权限不合规等初始化失败时，即使数据库可正常读写、窗口内已有已提交确认，continued Stop 仍不会写 `confirmed`。这使 §8.3 第 3 步“收取失败不影响后面的步骤”的覆盖不完整。

复现步骤：在临时状态库采用一个临时项目；以来源 `codex:synthetic-review` 写 TurnStarted（`00:01:00.000Z`）；提交该来源的 nothing_new 确认（`00:01:30.000Z`）；在临时可信根内创建权限为 `0755` 的 `cairn-spool` 目录；调用 `turn_ended`（`00:02:00.000Z`、`turn_key=turn-1`、`continued=true`）。所有时间均为 `2026-10-05` 的 UTC 时间。实测输出：

```text
continued=true; committed_confirmations=1; action=Allow; errors=[Spool(UnsafeDirectory)]; turn_decisions=0
```

**影响与分级**：本例确认本身仍在库中，没有误续跑、误认未提交确认或破坏安全目录检查；缺少的是异常路径下本可补记的回合判定，错误也已返回调用方。因此列为非阻塞建议，不据此否定任务中错误放行与最多续跑一次的验收。

**改法**：把暂存区打开错误也收集到 Report.errors；无法取得安全 Spool 时不做文件访问，以 Unknown 对待 pending，仍在数据库事务内检查 confirmed / continued 并记录能够确定的结果。保持任何错误时禁止续跑、不消耗续跑请求键。可补一个临时目录权限错误的定点回归，核对已提交确认仍被记录且原始错误保留；不需要修改 spool 的安全检查。

### 对完成记录“拿主意的地方”的逐项评价

1. **continued / confirmed / pending 的优先级，以及 `no-key:<窗口起点>` 的 skipped 键：同意。** 对应 `turn.rs:76–89,185–220`。缺 ID 的重复事件可能留下不同 skipped 键，但不会请求续跑；这是 `9f6f20e` 已明确接受的降级规则，不应再靠窗口构造续跑键。
2. **完全没有可观测起点时带错误放行、不编造时间：同意。** 对应 `253–267` 行。此时缺少安全判定依据，符合 §8.6。既有有来源/有会话的降级路径均已测试；本轮没有另造全排列验证。
3. **会话起点取最早 session_started，缺失时取 source.first_seen：同意。** 对应 `255–259` 行。前者保留同一来源会话的最初起点，后者复用已有观测；它们只在没有 turn_started 和既往判定时启用，不覆盖更具体的窗口。
4. **确认与 pending 都包含窗口起点：同意。** 对应 `270–275` 行及 `ingest::pending` 的既有契约。两个查询使用相同的 UTC 毫秒时间下界；既有测试覆盖起点、窗口前及其他来源。
5. **pending 独立给 100 ms，收取仍为 50 文件 / 300 ms：同意。** 对应 `159,165–171` 行。这是有界观测的局部选择，没有扩充 2c 收取预算；不是对操作系统调用硬截止的承诺。
6. **短 IMMEDIATE 事务重查采用、窗口和确认；窗口变化时按 Unknown：同意。** 对应 `179–198` 行。旧观测不被冒用到新窗口，事务内唯一键保证并发请求至多一个；没有引入先查再写的去重竞争。
7. **2c 查询返回 Unknown 时给固定 PendingUnknown，不猜底层原因：同意。** 对应 `175–177` 行。现有三态接口没有原因可传，固定错误能让阶段 3 留痕。因重查窗口变化而构造的 Unknown 属于保守判定，不等于查询接口报错。
8. **收取报错后继续观测，但所有错误均禁止续跑且不消耗请求键：同意原则，初始化失败的覆盖建议见第 1 条。** 对应 `159–161,199–203,228–230` 行。既有事务失败测试通过，能记录 pending / confirmed，并在故障解除后保留尚未使用的续跑机会。
9. **本模块用传入收到时间，2c 自身时间约定不改：同意。** 新写的来源、事件和判定使用 context.at；确认时间仍由 2c 使用暂存 created_at。没有把收取发生时间误作保存确认时间。
10. **pending 查询仅为有预算观测，不承诺跨文件系统/数据库原子快照：同意。** 这是 2c 接口已明确的边界。事务内重查确认缩小竞态窗口，但本轮未证明、实现也未声称能锁住并发 save 的发布；不要求 2e 在本任务中改变该接口或已定设计。
