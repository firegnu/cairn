# 任务：交叉审查 P1-27 接入 canter 的实现

2026-11-11，cairn/main 交给 cairn/review-canter（Codex，gpt-6-astra / xhigh）。
路由：碰要害 / 交叉审查要（实现任务的路由）；审查者用户 11-11 定换成 Codex（「审查换成 Codex」）。
类型：调研
依据：本轮只读审查分支 `p1-27-canter` 的改动，给出意见；不改代码。
提示：区分已查证事实、推测和未知，给出直接依据；结论限定在本轮调查范围。
你是被委派的 agent：照本文件做，不要再开别的 agent。

本文件在主仓库 `/Users/firegnu/Developer/personal_projs/cairn/docs/任务/P1-27-接入canter-交叉审查.md`，**不在**你的审查 worktree 里。你只写这一个文件。

## 背景

cairn/dev-canter-1（Codex）在分支 `p1-27-canter`（提交 `e9f5bef`）上，照 `docs/任务/P1-27-接入canter-实现.md` 把 canter 接成 cairn 的第三家 agent：hook 适配、会话记录解析器、install/uninstall、status 一项、数据库版本 6（重建 `hook_seen`），以及 canter 断线重连那次（`accepts_additional_context: false`）不注入。主控已看过 diff，重跑 `cargo test --all-targets`（220 项通过）和 clippy（无警告）。

## 先读

- `AGENTS.md`（规矩、隐私、编译目录）。
- 实现任务：`docs/任务/P1-27-接入canter-实现.md`（“要做的”“怎么算做完”“不要做”）和末尾的完成记录。
- `docs/DESIGN.md`：D16、§6.2“版本 6”、§7.1 `agents.canter.installed`、§8.8 canter 那一条、§10.5 整节、§14 R28–R30。
- canter 的公开接口（只读）：`~/Developer/personal_projs/canter/docs/DESIGN.md` 的「钩子（P2-153）」「会话记录的公开读取约定（P2-281）」。`~/.canter` 下任何东西都不读。

## 要审查的代码

- 你自己的 worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-p1-27-canter`（detached 在 `e9f5bef`）。**只读：不改、不提交、不切分支。**
- 改动：`git diff main...e9f5bef`（在主仓库或审查 worktree 里都能看）。
- 可以跑测试：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target/review-p1-27-canter cargo test --all-targets`，以及针对你怀疑之处的单个测试。验证照“碰要害”那档：标准测试加你认为必要的边角，不录屏、不做覆盖矩阵。
- 不读本机真实的会话记录、真实的 cairn 数据库，不写任何用户配置，不运行真实的 `cairn install`，不开真实 agent。

## 重点看

1. 版本 6 迁移（`store/schema.rs` V6、`store.rs`）：旧行是否完整保留；版本 5 的库只读打开（`status`）照常；升级在事务里、中途失败不会留下半截表；比当前新的库照旧拒绝。
2. Claude、Codex 的行为是否完全没变：`hook.rs`、`install.rs`、`status.rs`、`session.rs` 的共用路径（包括 `Paths::config` 改成返回 `Result`、`start_with_waiters` 多的参数）。
3. 重连那次（`accepts_additional_context: false`）：确实不渲染、不写 `injections`、不改 `ledger_injected_version`、不输出，事件和 `hook_seen` 照记，等待进程照起；`true` 和没有这个字段照旧。
4. 子会话（带 `agent_id`）：确实什么都不写、不输出，且只对 canter 生效。
5. canter 安装：配置目录不存在拒绝、不建目录；别的内容原样保留；解析不了拒绝；uninstall 只删 cairn 的条目；`status` 在没有 canter 时不报错、另外两家的字段不变。

## 输出

追加到本文件末尾「## 审查意见」。先写一句结论（可以合并 / 改完再合并），然后每条意见写级别（必须改 / 建议改 / 可以不改）、位置、问题、改法，最后对完成记录里的取舍逐条表态。

「必须改」只给两种：实现任务“怎么算做完”没达到；碰到的要害上有真实缺陷（会丢数据、并发出错、泄露或越权、核心规则算错）。其余一律「建议改」。

## 不要做

- 不改代码、不提交、不切分支；只写本审查文件。
- 不要按项目名或路径批量杀进程（`pkill -f cairn` 这类）。停自己起的进程用起的时候记下的 PID。
- 碰到要拿主意的事，停下来，在回复里写清问题和你的建议，然后结束这一轮，等主控回复。不在自己窗口里问用户。

## 做完

回复里只写结论和各级意见的条数。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 审查意见

可以合并；本轮未发现实现任务验收未达到或要害路径上的真实缺陷。

2026-11-11，cairn/review-canter。审查对象固定为 `e9f5befa59c1c193fcb1d4b617de786a1a95dfc6`，差异基点为 `aa6602f275cee724871cd9c5d9bc8e2ff0fbcec5`。意见计数：必须改 0 条，建议改 0 条，可以不改 0 条；下面是查证依据和对已有取舍的表态，不另算整改意见。

### 查证依据

- **版本 6 迁移**：`crates/cairn/src/store/schema.rs:164` 仅重建 `hook_seen`，四列逐列复制，保留原主键、外键和事件约束，只扩展 agent 约束。`store.rs:157` 在立即事务中重新读取版本，执行迁移及版本号更新，失败随事务回滚。`tests/store.rs:1260` 比较版本 5 各表旧行和除 `hook_seen` 外的结构；既有并发升级测试此次也走到版本 6。只读版本 5 和拒绝版本 7 的测试通过，包括未来版本在 WAL 中时不改写数据的检查。
- **失败回滚额外验证**：用冻结的 v3/v4/v5 SQL 夹具在临时目录建版本 5 库，种入两个项目、Claude/Codex 各四种事件，共 16 行；在 `meta` 上加触发器，使迁移最后的版本号更新报错。运行本次构建的 `cairn adopt` 后，整库 SQL 导出与执行前相同，版本仍为 5，`hook_seen_v6` 不存在，`status --json` 仍可读。删除故障触发器后重试升到 6，16 行完整保留，外键和完整性检查通过，新增 canter 行可由 status 读出。脚本首次因外部 sqlite3 创建的 SHM 权限过宽被安全检查拒绝；改用私有 umask 077 后完成上述验证，这次夹具错误不作为产品缺陷或迁移失败证据。
- **重连不消费注入**：`hook.rs:167` 只对 canter 的显式 `false` 关闭注入；`session.rs:109` 先启动等待进程，再提交登记事务并返回，渲染在其后，`injections` 和 `ledger_injected_version` 的写入也在其后。来源冲突时只更新 `last_seen`，保留首次开局时间。`tests/waiter.rs:446` 验证无输出、事件与 `hook_seen` 照记、等待进程实际启动、注入状态不变、下一次发话补入且不重复。`tests/hook.rs:1119` 验证 canter 的 `true`/缺字段，以及 Claude/Codex 的 `false`，均保留正常注入行为。
- **子会话**：`hook.rs:49` 用字段是否出现识别 `agent_id`，包括 null；`hook.rs:92` 在解码其它元数据、取得数据库/暂存区路径和启动进程之前返回，并明确限定 canter。`tests/hook.rs:1032` 检查各相关表、暂存区及损坏数据库场景。本轮额外用四个带 null `agent_id`、其余元数据故意非法的 canter 事件核对整库导出不变、退出 0、无 stdout/stderr 和错误日志；相同 `agent_id` 加 `accepts_additional_context: false` 的 Claude/Codex SessionStart 仍登记并输出上下文。
- **共用路径与安装/status**：Claude/Codex 的 hook 命令文本、超时和权限规则未改；`session::start` 仍显式传入 true。`install.rs:18` 构造 canter 路径时不验证它，`Paths::config` 只检查当前选中的路径，因此 canter 路径有问题不会让另外两家初始化失败；另外两家的路径仍沿用原有 `dir` 校验。canter 安装前检查父目录，写入时不建该目录；沿用原文合并、备份、确认和原子替换，卸载只移除识别为本工具的条目，不加 timeout 或 save 规则。`tests/install.rs:238`、`:586`、`:639` 覆盖原文保留、重复安装、卸载、目录缺失、解析失败及另外两家 status 对象不变。`status.rs:96` 仅吞掉 canter 配置错误并报未安装，四个 `last_seen` 键始终补齐。额外迁移验证也比较了升级前后 Claude/Codex 的完整 status 对象，保持一致。
- **会话内容与来源**：`transcript.rs:333` 只提取公开的两类 completed 消息，用户文本片段按顺序换行连接；助手缺省/completed 正常读取，interrupted 明示未说完，未知状态及其它项目跳过。`sidecar.rs:582`、`:685` 同时接通记账和摘要。合成夹具测试覆盖增量及末尾不完整行、忽略工具/镜像/撤回标记、无对话增量不调用模型、摘要和账的 canter 来源及 list 输出。未发现新增的对 Claude/Codex 解析路径的改动。

### 本轮验证及边界

在约定的独立编译目录 `.target/review-p1-27-canter` 前台完成：

- `cargo test --all-targets`：220 项通过，无失败。
- `cargo clippy --all-targets -- -D warnings`：通过，无警告。
- 上述临时数据库及 hook 边角脚本：通过，临时目录已清理。
- `git diff --check main...e9f5bef`：通过；审查 worktree 保持干净、HEAD 未变。

结论限于指定提交、公开约定、静态检查和合成测试。没有运行真实 agent、真实 install 或读写真正的会话/数据库/用户配置。真实 canter 是否将上下文完整交给模型、模型沙箱内 save 是否可用，本轮没有验证，仍由主控照原安排另派实测；不能把本结论当作该实测已经完成。

### 对完成记录中取舍逐条表态

1. **途中接入 P2-284、以更新后的 main 设计实施：同意。** 当前实现与 DESIGN §10.5 的 false/true/缺字段三种约定一致，没有重新引入已否决的重连猜测规则。
2. **内部增加 SessionStart 注入参数，保留公开 `SessionStarted`：同意。** 参数落点正确，避免重连渲染与记已注入，同时保持既有公开调用和另外两家的默认行为；无需为这次适配扩大公开接口。
3. **子会话按 `agent_id` 字段存在即跳过：同意。** 符合“带这个字段一律不处理”的要求；采用存在标记而不要求字符串值，能够在其它元数据无效时也提前退出。
4. **只读公开用户/助手消息、回退撤回不撤销、工具输出不读，继续使用增量字节位置：同意。** 与用户已定的 §8.8、R29/R30 一致；公开记录只追加，因此现有按文件位置续读在本轮范围内适用。interrupted 保留但标明未说完，符合 cairn 的明确设计。
5. **canter 可选、安装不建目录、不写 timeout、不加 save 放行规则：同意。** 是 §10.5 已定边界；status 只观察配置和触发记录，不探测服务端或版本，不能据 installed 推断实际注入已成功。
6. **不跑真实 agent、不安装、不升程序版本、不改 HANDOFF、不合并推送：同意。** 都是本实现任务的明确范围；真实联调和发布留给主控，不要求在本分支补做。
7. **两处测试夹具修正：同意。** adopt 已建空暂存区，改为检查为空和相关表无变化，没有放宽目标行为；rusqlite 整数读取改为 i64 是夹具编译修正，不把编译错误算作有效 RED 的表述正确。本轮复跑证明当前测试通过，但不能独立还原实现者此前每次 RED 的执行过程。
