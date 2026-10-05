# 交叉审查：3b install / uninstall / status

2026-10-05，cairn/main（主控）交给 cairn/dev-review-install（Codex，重：gpt-6-astra / xhigh）。
类型：调研
提示：区分已查证事实、推测和未知，给出直接依据；结论限定在本轮调查范围。
你是被委派的审查者：只读审查，不要开别的 agent。**本文件在主仓库工作区（`/Users/firegnu/Developer/personal_projs/cairn/docs/tasks/`），不在你的审查 worktree 里；你只写本文件这一个文件。**

## 背景
- cairn/dev-install（Codex）在分支 `p3b-install` 上实现了 install / uninstall / status，提交 `58186f5`。任务书 `docs/tasks/P3b-安装.md`（审查 worktree 里也有，末尾有完成记录）。
- 主控已审过：重跑 `cargo test --all-targets`（99 项）、clippy、`git diff --check` 通过；确认真实 `~/.local/share/cairn` 不存在、真实 Claude / Codex 配置里没有 cairn 条目。

## 先读
- `AGENTS.md`（规矩一节，尤其"不改用户真实配置"）；任务书与完成记录；任务书"先读"里列的 DESIGN 和实测报告章节。

## 要审查的
- 审查 worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-p3b-install`（detached，指向 `58186f5`）。改动范围：`git diff main...HEAD`。**只读：不改、不提交、不切分支。**
- 可以跑该分支的测试和 clippy（命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`），可以在**临时 HOME**（同时设临时 `XDG_*`、`CODEX_HOME` 及 Claude 配置目录变量）下直接运行编译出的 `cairn install/uninstall/status` 核实。
- **绝对不要在真实 HOME 下运行 install / uninstall**；不要读写真实 `~/.claude*`、`~/.codex`、`~/.local/share/cairn`。不要运行 claude、codex，不要开 agent；不要读真实会话记录、工具记忆、Corral 事件文件。不用 Python。只做定点核实，不做覆盖矩阵。

## 重点看
1. **不破坏用户配置**：合并只动 cairn 自己的条目；用户已有的同事件 hook、其他设置、key 顺序都保留；识别"自己的条目"的方式会不会误删用户手写的、长得像的条目；原文件是坏 JSON、是符号链接、权限特殊时怎么处理；写入是否原子、备份是否可靠且不覆盖。
2. **两种工具的配置格式**：生成的 Claude `settings.json` hooks 与 permissions 结构、Codex `hooks.json` 结构（含 `additionalContextLimit`、SessionEnd 超时）是否和 DESIGN §10、实测报告、`p1-lab/configs` 里实际生效过的格式一致；命令一律写稳定路径。
3. **稳定路径与升级**：软链接的创建 / 更新 / 失效处理；hook 文本在升级（软链接换目标）后是否保持不变，从而 Codex 不需重新信任。
4. **确认与 dry-run**：`--dry-run` 真的不改任何文件（包括软链接）；非终端且没 `--yes` 时拒绝；uninstall 后与安装前等价。
5. **status**：只读、不建库；各状态报告准确；不越权读真实路径（测试中）。

## 输出
追加到本文件末尾「## 审查意见」：先写一句结论（**可以合并** / **改完再合并**）；每条意见写级别（必须改 / 建议改 / 可以不改）、位置、问题、改法。**"必须改"只给两种情况**：任务书"怎么算做完"没达到；或者碰到的要害上有真实缺陷（会破坏或丢失用户配置、写错格式导致 hook 不生效或误生效、扩大权限、dry-run 有副作用）。其余一律"建议改"。能给复现步骤的写上。最后对完成记录里"拿主意的地方"逐条表态。

## 回复
回复里只写结论和各级别条数。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 审查意见

**改完再合并。** 必须改 2 条，建议改 1 条，可以不改 0 条。

审查者：cairn/dev-review-install（Codex），2026-10-05。范围限定为 detached 提交 `58186f5161897abd7c159ff7d429bbf7fcd1f4ef` 相对 `main` 的改动及其直接调用路径；未改源码、测试、分支或提交。以下 R1、R2 是在隔离 HOME 中复现的用户配置保护缺陷，不是推测。

### R1 — 必须改：复合命令会被误认作自身 hook 并删除

- **位置**：`crates/cairn/src/install.rs:61–70`（`own_command`），`136–143`（据此删除 handler）；`status.rs:28–33` 也使用同一判定。
- **问题**：`shlex::split` 只处理词与引号，不能据此断言输入是一条直接执行的 shell 命令。命令 `/usr/bin/true;/opt/user/cairn hook claude` 被拆成三个词，第一个词在 `Path` 看来又是绝对路径且 basename 为 `cairn`，因此被认领。实际上它是用户的复合命令，包含独立的前置动作；完成记录承诺“不处理包装脚本或复合命令”，这里没有守住。
- **已查证复现**：临时 `CLAUDE_CONFIG_DIR/settings.json` 写入 `{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/usr/bin/true;/opt/user/cairn hook claude","timeout":17}]}]},"theme":"dark"}`，在同一隔离环境运行 `cairn uninstall --agent claude --yes`，退出码为 0，配置变成 `{"theme":"dark"}`。用户 handler 被整条删掉；复现只调用 uninstall，没有执行这个合成 hook。安装路径同样使用这段删除逻辑，再补入默认 handler，会丢失用户的前置命令及原设置。
- **改法**：认领必须限于确实支持的“直接执行 cairn”命令语法，保守保留含未引用 shell 控制符或展开的命令；不能仅凭 `shlex` 的三个词及 basename 判断归属。若保留旧绝对路径迁移能力，也应明确限制这一语法，保留合法空格/单引号路径支持。补一条隔离回归检查，确认 install/uninstall 均原样保留该用户复合 hook；status 也不应把它当成自身直接 handler。
- **级别依据**：实际丢失用户 hook，违反验收 2、4 及“只动自身条目”的要害边界。备份存在不能代替不误删。

### R2 — 必须改：重复 JSON 键会使无关设置的值在写回后改变

- **位置**：`crates/cairn/src/install_json.rs:15–34`，以及 `install.rs:110` 的输入解析。
- **问题**：当前解析接受重复键。`Value` 与 `BTreeMap<String, &RawValue>` 都只留下某一键最后出现的值，但 raw fragment 的前缀还会包含被丢弃的早先成员；随后按 `Value` 的键顺序拼接，会挪动这些隐藏成员的先后顺序。这不只是格式变化，会改变配置的解析结果。
- **已查证复现**：临时 settings 原文为 `{"theme":"light","hooks":{},"theme":"dark"}`。运行 `cairn install --agent claude --yes` 后退出码为 0，输出文件开头变成 `{"theme":"dark","theme":"light","hooks":...}`。同一个 `JSON.parse` 在安装前读到 `theme="dark"`，安装后读到 `theme="light"`；与 cairn 无关的合成设置被反转，原始键顺序也未保留。此问题发生在合并渲染层，不依赖真实 agent 是否使用这个合成字段。
- **改法**：最小且稳妥的处理是，在任何配置/备份/软链接写入前递归检测并拒绝重复对象键，报清楚错误并保留原文件；不要尝试用当前 map 丢重后的偏移继续拼接。补一条上述输入的回归检查，验证拒绝时配置字节不变且没有安装副作用。若决定支持重复键，就必须保留成员的完整有序序列及其原解析语义。
- **级别依据**：已证实修改用户无关配置的值，违反验收 2 的原文保留要求，属于配置破坏。

### R3 — 建议改：说明安装/卸载对原有空容器的清理边界

- **位置**：`crates/cairn/src/install.rs:148–165`、`187–193`；`crates/cairn/tests/install.rs:343–354`。
- **问题及复现**：原配置 `{"hooks":{"Stop":[]},"permissions":{"allow":[]},"theme":"dark"}`，先 install 再 uninstall 后变成 `{"theme":"dark"}`。当前“没有自身条目时直接卸载”的测试保住了空对象，却没有覆盖安装前就存在空容器的往返情况。实现无法区分原有容器与本次安装新增容器，删除最后一条自身 entry 时一并删掉了原有空容器。
- **改法**：至少在完成记录中明确这条清理规则及“等价”的范围，补充这个小型往返用例；若要求结构也保留，应采用能保留原有空容器的策略。无需为此扩成配置迁移框架。
- **级别依据**：目前只证实结构变化，没有证实这些空容器缺失会改变 hook 或权限的实际语义，故不作为合并阻断项。

### 已核实与验证边界

- 本轮前台运行 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --test install`：8 项全部通过；`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo clippy --all-targets -- -D warnings` 通过；`git diff --check main...HEAD` 通过。未另跑全量 99 项，任务背景中主控的全量结果不冒充本轮重跑结果。
- 已读任务书、完成记录、DESIGN §7/§10/§12 及相关决定、实施计划、背景记录、hooks 资料、HANDOFF；对照阶段 1 报告 §7/§14.4 及两个指定配置样例。生成的四事件 hooks、Claude settings 中的单条 save 权限、Codex handler 上的 `additionalContextLimit: 6000` 结构一致；SessionEnd 的 1 秒/3 秒符合设计。安装不写 `config.toml` 或 sandbox/approval 设置。
- 现有 8 项集成测试通过，覆盖普通用户配置片段保留、重复安装、稳定链接更新后配置字节不变、卸载/备份、dry-run、非终端确认拒绝、坏 JSON/配置符号链接/稳定路径普通文件占用拒绝、POSIX `0640` 权限保留，以及 status 的安装、失效链接、采用、无库和暂存计数状态。特殊权限结论仅覆盖 POSIX mode，未验证 ACL。
- 代码核对：配置使用同目录临时文件、同步后原子替换；备份使用带时间和随机后缀的独占临时文件并保留。status 在收取入口之前分流，沿用只读 Store 和只检查已有目录的 Spool::inspect，不建库、不收取。未做掉电或并发修改配置的故障实验，不把顺序执行通过扩大成这类保证。
- 额外只做上述三个定点核实，全部使用合成配置；每次子进程采用显式隔离环境，设置临时 HOME、XDG_DATA_HOME/STATE_HOME/CONFIG_HOME/CACHE_HOME/RUNTIME_DIR、CODEX_HOME、CLAUDE_CONFIG_DIR、TMPDIR，并禁用 Git 系统/全局配置。未运行真实 Claude/Codex、未启动其他 agent、未在真实 HOME 安装或卸载，未读取真实 agent 配置/会话或 cairn 数据库。合成现场及备份留在 `/tmp/cairn-review-p3b-4a7muz/`（`compound`、`duplicate`、`empty`），供主控定点复核。

### 对完成记录“拿主意的地方”逐条表态

1. **配置目录环境变量、空值回退、拒绝相对路径：同意。** 路径选择与隔离测试一致；没有因此扩大写入范围。这里确认的是实现的路径选择与本轮隔离测试，不宣称重新做过真实工具的环境变量实测。
2. **用直接绝对路径命令识别自身、支持 shell 引号、不处理包装/复合命令：方向同意，当前实现不满足，须按 R1 收紧。** 旧路径迁移不能成为删除用户复合命令的理由；不要求支持任意 shell 语法。
3. **卸载保留稳定软链接：同意。** 任务明确授权实现者决定；保留链接避免干扰另一 agent 和已有命令引用，卸载输出已告知，也未删除二进制、状态库或暂存文件。
4. **status 接受只读 SQLite 的 WAL/SHM 辅助文件行为：同意并沿用主控已定边界。** 未要求改 immutable 或重新打开该决策；现有验证确认主数据库字节不变、暂存条目不被收取，缺库时不创建。

## 复核意见

**可以合并。** 必须改 0 条，建议改 0 条，可以不改 0 条；原 R1、R2 已修复，R3 已按主控决定落实，本轮未发现新增问题。

2026-10-05，cairn/dev-review-install（Codex）复核。目标为 detached 提交 `42cdd9ff728595f0e1303ac67da6886f07411833`，只检查 `58186f5..42cdd9f` 的四个变更文件及直接影响，没有重复全量审查或重新讨论已定取舍。

- **R1：通过，关闭。** `crates/cairn/src/install.rs:75–108` 在 shlex 分词前检查引号、转义、未引用控制符及展开；hook 和权限条目认领复用此检查。原场景 `/usr/bin/true;/opt/user/cairn hook claude` 独立复跑：status 的 Stop 为 false；先直接 uninstall 时原配置逐字不变；再 install/uninstall 后，用户 handler 原文保留，解析后的配置与原配置一致。`tests/install.rs:358–409` 同时覆盖两种 agent 的运算符、展开、注释及合法单引号路径；现有空格/单引号稳定路径用例也通过。未执行任何合成 hook 命令。
- **R2：通过，关闭。** `crates/cairn/src/install_json.rs:10–57` 先验证完整 JSON 及嵌套限制，再递归检查对象、数组中的解码后键名；`install.rs:149` 在合并及所有写入之前调用它。原文 `{"theme":"light","hooks":{},"theme":"dark"}` 独立复跑 install/uninstall，均退出 1，报 `duplicate object key: "theme"`；前后目录快照相同，原文件字节和权限不变，无新增备份、目录或软链接。`tests/install.rs:414–447` 对嵌套对象、数组和 Unicode 转义重名键的检查也通过。
- **R3：按决定落实，关闭。** `docs/tasks/P3b-安装.md:98` 已说明删除自身最后一条 entry 时清理空容器，不区分其原先是否存在；“等价”限定于 hook/权限语义。`tests/install.rs:451–463` 使用原审查输入，明确断言安装/卸载后仅剩 `{"theme":"dark"}`，本轮通过。生产清理逻辑未变，不再要求保留原有空容器。
- **增量影响：未发现新问题。** 改动限于命令认领、重复键前置校验、三项回归及返工记录；保留原 JSON 增量渲染和写入流程，没有改 hooks 定义、稳定路径、确认流程或 status 的数据库/暂存区行为。检查了新字面量判定对合法引用路径的影响，以及重复键校验在写入之前的调用顺序；未新增待改意见。

本轮所有验证均在前台等待结束：

- `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --test install`：11 项全部通过，含新增 R1–R3 和原 8 项相关回归。
- `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo clippy --all-targets -- -D warnings`：通过。
- `git diff --check 58186f5..42cdd9f`：通过。
- 上述两个原场景的独立定点复跑使用临时 HOME，显式隔离 XDG_DATA_HOME/STATE_HOME/CONFIG_HOME/CACHE_HOME/RUNTIME_DIR、CODEX_HOME、CLAUDE_CONFIG_DIR、TMPDIR 及 Git 配置；现场保留在 `/tmp/cairn-recheck-p3b-sOuSkR/`。没有运行真实 agent，没有读写真实配置、会话、工具记忆或 cairn 数据库。

未重跑全量 102 项，不将开发方的全量记录算作本轮独立验证。只向本审查文件追加本节；审查 worktree 的源码、测试、分支和提交均未修改。
