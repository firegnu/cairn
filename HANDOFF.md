# 交接

2026-10-12，由 cairn/main（主控，Claude Code）更新。10-10 更早 paddock/main 按用户的话直接在本仓库做了 F2 并改过本文件，那部分留在“10-10 更早”一节。

## 本次会话（10-12，cairn/main）

**摘要**：早些时候把记忆系统设计草案和 P1-01 草稿提交了（`7c90a4d`）。之后全是讨论，外加一件调研（P1-03）和一件实测（P1-04）。用户定了改走旁路模型路线。没动代码，`docs/DESIGN.md` 没动。

**完成的**

- **P1-03 调研已合并**（Codex 做，主控审过，返工一处，已推送，worktree 和分支已清，dev agent 已关）：报告 `docs/调研/会话接续的写入时机.md`；任务书、完成记录、主控审查在 `docs/任务/P1-03-调研会话接续的写入时机.md`。报告的结论：别家也有“什么时候写会话状态”这个问题；中间的做法看到三种（程序持续留原始材料、模型隔一段再提炼；同一会话持续更新一份摘要；按进展更新同一个工作项）；没找到无条件两头都顾住的。报告不含 cairn 的设计，所有对象都没实测。

- **P1-04 实测已合并**（Codex 做，主控对着实验目录的证据审过，已推送，worktree 和分支已清，dev agent 已关）：报告 `docs/调研/旁路摘要可行性实测.md`；任务书、完成记录、主控审查在 `docs/任务/P1-04-实测旁路摘要走不走得通.md`。本机 Claude Code 2.1.296、Codex 0.162.1 上，“读 hook 给的会话记录、调已登录的 CLI 写六节摘要”走得通；Codex 被 `corral stop` 关掉时和两家被强杀时没有会话结束事件；旁路会话照样触发所在目录的 hook。路由判要交叉审查，用户选了省掉（“跑完吧”）。任务书里为这件活开过一个隐私例外：只读实测自己开的测试会话的记录。
- **旁路摘要设计草案已起草**：`docs/旁路摘要设计草案.md`（主控写，**未提交、没生效**，`docs/DESIGN.md` 没动）。只有“走旁路模型路线”一条是用户定的，其余是主控的提议；第 7 节七个问题等用户定。

**用户说的**

- “现在首要的问题就是记得太频繁了”：本会话前四个回合各存了一条全量记录，8 分钟 4 条。这件事排到了记忆系统设计草案前面。
- 用户指出 HANDOFF 是另一个极端：不主动 handoff 就不记。
- “我同意旁路模型这个路线。现在的路线记得太多了，会造成混乱”：cairn 读会话记录，由旁路模型事后写摘要，取代 agent 每回合手写的记录。这要推翻 D6、D12、DESIGN §12 和 `AGENTS.md` 隐私一条里“不读 `transcript_path`”那句，正式文档都还没改。
- 实测开了 7 个测试会话，用户嫌多（“这在干什么？不停拍agent”）。以后派实测先说清会开多少个，任务书别写重。

**本仓库用 cairn 的观察**（主控一个会话的样本；主控熟悉 cairn 内部，不代表一般 agent）

- 开局注入够用：只靠最新一条记录就能答“下一步是什么”，现场对比和 git 相符。本文件全文也读了，回答里没用上只有它才有的内容。
- 注入了三条记录，两条是旧的：那两个来源的最后一条已被别的会话取代，注入退回显示它们更早、没被取代的记录，里面有已不成立的话。照 D9 的字面符合设计。
- 每回合存不存，依据是开局注入的“接续约定”，标准只有“本回合是否产生了下一次会话需要接续的内容”；正文模板里有“未落盘的讨论要点”一节，所以纯讨论的回合也会判成“有”。
- 每条记录都是全量重写，约一半照搬上一条；同一会话的旧记录在 `cairn list` 里仍显示“可见”（注入时同一来源只给最新一条）。

**主控提过、用户还没表态的**（都不是决定，谈定了写进 `docs/DESIGN.md`）

- 太频繁怎么改：先提过三条不读对话的补丁（同一会话再存算新版本、只写变了的节、门槛改严），用户选了旁路模型路线之后不再往下推；其中“一个会话一份、再写算新版本”进了旁路摘要草案。
- 设计草案：主控给第 12 节的第 4、7、8 问补了建议；指出 §1.2 的定位用户没表态；反问用户“智能”指望 cairn 自己做哪件事（可能改 M2）。

## 10-11 的会话（cairn/main）

**摘要**：用户提出让 cairn 仓库自己也用 cairn（自举）。先改了 AGENTS.md 的隐私一条，随后用户自己执行了 `cairn adopt`。没有动代码。

**完成的**

- AGENTS.md“隐私”一条末尾加了一段（`e0d3949`，用户看过措辞后同意，已推送）：本仓库自己采用 cairn 属于正常使用，通过 cairn 的 hook 和命令读写本项目的记录可以；不直接打开数据库文件、不看别的项目的记录、排查问题仍在隔离目录用合成输入复现，这三样照旧。
- **本仓库已采用 cairn**（用户 10-11 自己执行）。`cairn status` 核对：项目已采用；Claude 的 UserPromptSubmit 在采用后的第一条消息就记上了时间（08:56Z）；Codex 在本项目还没有触发记录。

- **本仓库存下了第一条记录**（`01M4N2X9ZYM5P9RZBPSPPGX7TC`，来源是本次会话）。主控没拿到来源 ID 所以没主动存，是回合结束时 Stop hook 要求续跑、给出 ID 后存的；之后每个回合都照约定确认。核对到的：
  - `cairn list` 里有这条，状态“可见”；用户在 paddock 的 Cairn 面板里也看到了。
  - `cairn show` 渲染出来的内容对：记录五节完整；之后存的几次“无新内容”确认没有顶替它；现场对比（HEAD 多 0 个提交、与 origin/main 持平、工作区干净）和实际相符。
  - `cairn status`：Claude 的 UserPromptSubmit、Stop 在本项目都有时间；SessionStart、SessionEnd 还没有。

**没做完、要跟进的**

- **开局注入已验（10-11 的下一个会话，本仓库第一个样本）**：新会话开局收到了 SessionStart 注入（来源说明、带本会话来源 ID 的接续约定、记录 `01M4N35YC9XN3X5SHHPMVGJD44` 的正文、现场对比），停点、已完成、下一步、待用户决定都和本节对得上，现场对比和 git 实际相符。“用户原话”一栏本文件里没有原文，没法核。同一来源有两条记录都“可见”时，注入只给新的那条。
- `cairn status`（10-11 09:04Z）：Claude 四种事件在本项目都有时间了，SessionStart 09:03:57Z，SessionEnd 09:03:40Z（上一个会话结束时记上的；paddock 里一直没见到 Claude 的 SessionEnd，这里见到了）。Codex 在本项目仍没有触发记录。

**新方向：把 cairn 发展成 agent 的记忆系统（10-11 用户提出并认可，先出设计，细节等用户详细讨论）**

- 用户：“我是想把你这块单独发展成agent的记忆和RSI之类的memory系统，包括跨agent的记忆共享等等功能。”
- 用户在看板上建了草稿 `docs/任务/P1-01-扩展成一个完整的记忆系统.md`，主控按讨论补了内容。**它还是草稿**（10-12 已提交）。用户看过调研后定了五条（“这个cairn我想发展成一个独立的记忆（智能）系统。可以独立和agent使用也可以和paddock之类的结合使用。”“按你的建议来，先出设计。明天咱们再详细讨论。”）：就在本仓库里长、仍是独立程序；cairn 自己先不调用模型；自我改进先做到“经验逐条沉淀、逐条更新”这一层；共享先做同项目里 Claude Code 和 Codex 之间、再做跨项目；P1-01 只出设计，实现另拆。
- **设计草案已起草**：`docs/记忆系统设计草案.md`（主控写，10-12 已提交，**还没生效**，`docs/DESIGN.md` 仍是权威，没动）。第 12 节列了八个要和用户定的问题。用户说 10-12 详细讨论。
- 用户要先调研再讨论。**P1-02 调研已合并**（Codex 做，主控审过）：报告 `docs/调研/agent记忆系统现状.md`，任务书、完成记录、主控审查 `docs/任务/P1-02-调研agent记忆系统现状.md`。报告不含 cairn 的设计。一处和本机对不上：报告按官方文档写 Codex memories 默认关闭，本机 `codex features list` 显示 `memories stable true`，原因没查。
- **任务文件的位置**（用户：“从这个任务统一到kanban。之前的就那样了”）：从 P1-01 起放 `docs/任务/`，之前的留在 `docs/tasks/` 不搬。`AGENTS.md` 里那一句已经改了，10-12 已提交。
- 用户不让查 `errors.log` 那几条（“先不查那几条”）；也不要去翻 paddock 的东西（“你查你自己的就行了”）。

**拿的主意**

- 和 paddock 一样，先跟全量 `HANDOFF.md` 并存：并存期间本文件仍是权威，cairn 的记录只作对照；本文件瘦不瘦由用户定。
- 采用的代价当时跟用户讲过：本仓库少了“没采用所以误跑会被忽略或拒收”这层保护（所有 worktree 都算已采用，派出去的 agent 靠 `CAIRN_DISABLE=1`、测试靠隔离目录）；主控每个回合多一次保存或确认，开销没量过；主控了解 cairn 内部，表现不能代表一般 agent。

## 10-10 的会话（cairn/main）

**摘要**：接手时交接还停在 10-07（“先别安装”），实际试点已经在跑；重读交接和 F2 改的设计后对齐了现状。随后按 paddock/main 转来、用户同意的需求做了 F3（`list --json` 和单条记录的公开约定，0.3.0），装到真实环境并通知了 paddock/main。

**完成的**

- **F3**（paddock P5-79 要的 cairn 这一半；任务书、完成记录、主控审查：`docs/tasks/F3-list-json与单条记录约定.md`）：
  - `cairn list --json [--limit N]`：只读（不收取暂存区、不建库、不升级库），输出 `{"total":…,"records":[…]}`，新的在前；`total` 是截断前的条数。
  - `cairn list --json` 和 `cairn show <ID> --json` 写进公开约定（DESIGN §7.1）。
  - 版本号 0.2.0 → **0.3.0**。Codex（`cairn/dev-list-json-1`，常规档）实现，主控审查通过。main 上 **119 项测试**通过，clippy 干净。
  - 已合并（`2cc973a`）、推送，worktree 和分支已清，dev agent 已关，收尾提交 `d3c093d`。
  - **0.3.0 已装到真实环境**（用户：“都做，装好后直接corral send给paddock”）：20:32 `cargo install --path crates/cairn --locked` 换掉了 `~/.cargo/bin/cairn`，稳定软链接照旧指向它。表结构没变，没备份数据库。
  - **已通知 paddock/main**（`corral send`，已送达）：字段名、格式、版本号。回执里 `merged_with_draft` 为 true，它的输入框里当时可能有没发出的草稿，已告诉用户。
- AGENTS.md“合并”一条改成合并后推送到 origin（`64776d7`，用户同意）。
- 两份 README 里 `cairn list` 的范围从“当前工作线”改准为“当前项目”（F3 顺带，原文不准）。
- 只读看了两次试点（没碰 paddock/main），结果见“下一步”第 1 条。

**没做完、要跟进的**（其余见“下一步”“悬而未决”）

- 新会话开局注入的内容准不准，还没看。
- `list --json` 在真实环境里只跑过 `--limit 0`（只看条数，返回 27 条）；带内容的输出只在隔离测试里验过，paddock 的 P5-79 是第一个真实使用方。
- `errors.log` 当天 9 条的来源没查（用户：“暂时不用了，paddock在跑，他也接了你，直接使用它测试吧”）。
- F2 的实现代码和任务、审查记录主控没读，动存储、hook 相关代码前要读。

**拿的主意**（理由在任务书和 DESIGN §7.1，以后改这些输出先看 §7.1）

- `list --json` 只读；没有数据库、没采用过、没有记录都给 `{"total":0,"records":[]}`，只有一种形状（和 `show --json` 的 `{"status":"no_data"}` 不一样）。
- 状态用 `deleted_at`、`replaced_by`、`retracted` 三个字段表示；现有的中文文字数组 `status` 不进约定。
- `show <ID> --json` 没改成只读（paddock 只要求写明）：它照旧先收取暂存区，会写数据库。
- 列表里更正、撤回、恢复也各算一条（和文字版 `list` 一致），调用方靠 `kind` 区分。
- 没做交叉审查：只读命令加输出，不动表结构和写入路径，影响面定为“改行为”。
- 别的主控转来的需求怎么接：第一次（F2 那两项）主控用提问对话框向用户核实，用户取消并改让 paddock/main 自己做；F3 这次依据 paddock 任务文件里的用户原话直接开工，只把装新版和 `corral send` 两步留给用户点头。对用户现有 agent 执行 `corral send` 仍然照规矩不做，这次是用户明确放行，只限这件事。

**涉及的文件**

- `docs/tasks/F3-list-json与单条记录约定.md`；`docs/DESIGN.md` §7、§7.1。
- `crates/cairn/src/cli.rs`、`crates/cairn/src/commands.rs`、`crates/cairn/tests/cmds.rs`、`crates/cairn/Cargo.toml`。
- `README.md`、`README.zh-CN.md`、`AGENTS.md`（`CLAUDE.md` 是指向它的软链接）。
- 需求原文（paddock 仓库，只读）：`../paddock/docs/任务/P5-79-Cairn标签改成记录列表.md` 的「cairn 给的东西」一节。

## 10-10 更早（paddock/main 在本仓库做的 F2）

- 起因：paddock 的 Cairn 面板（paddock P5-55）只知道 hook 装没装，分不出“装了但没触发”；它调用的三条命令的输出也没写成约定。用户先让 paddock/main 把这两项需求用 `corral send` 交给 cairn/main；cairn/main 分析后给了 A（按项目，要升表结构）、B（按 agent 全局）两条路并用提问对话框等用户选。用户取消了那次提问，改口：“我取消了，我觉得你直接再cairn中干吧，之后更新他的handoff文件，以便下次能够正确识别最新的情况。另外我觉得问题不要最小化操作。就是一次解决。之前cairn中的数据我认为不重要。”所以这件活没有经 cairn/main 派发，AGENTS.md 的“主控不自己写功能代码”这一次照用户的话没有执行。
- 做了 **F2**（任务书和完成记录：`docs/tasks/F2-hook最近触发与公开约定.md`；交叉审查三轮到“可以合并”：`docs/tasks/F2-hook最近触发与公开约定-交叉审查.md`）：
  - 表结构升到**版本 2**：加 `hook_seen`（项目，agent，事件 → 最近一次被处理的时间）。这是 cairn 第一次改表结构。版本 1 的库在读写打开时升级，只加表；只读打开不升级、照样能读（DESIGN §6.2 末尾）。
  - 四种 hook 事件处理时各记各的；`cairn status --json` 每家多一个 `last_seen`（四个事件名 → 时间或 `null`），文字输出每家多一行“本项目最近触发”。记法和三个限制（没采用不记、`CAIRN_DISABLE=1` 不记、处理出错不记）在 DESIGN §8.7。
  - **公开约定**（DESIGN §7.1，只加不改）：`status --json` 的 `agents.*.installed`、`agents.*.last_seen`、`project.status`、`spool.pending_json`；`show --json` 的两种形状；`adopt` 的退出码和输出。以后动这些输出先看 §7.1。
  - 顺带修了一个原来就有的空隙：SessionStart 只在只读探测时查采用状态，现在拿到写事务后再查一次（DESIGN §8.1 第 2 步）。
  - 版本号 0.1.0 → **0.2.0**。
- 合并进 main（`8615769`）、推到 origin、worktree 和分支清掉、审查 agent 关掉、收尾提交（`ddfd433`）。main 上 **113 项测试**通过，clippy 干净。
- **0.2.0 已装到真实环境**：`cargo install --path crates/cairn --locked` 换掉了 `~/.cargo/bin/cairn`（稳定软链接 `~/.local/share/cairn/bin/cairn` 指向它，hook 配置不用改）。装之前把真实数据库整份复制到 `~/.local/state/cairn-backup-20261010-before-v2/`（没打开看内容）。装好后在 paddock 仓库里跑了一次 `cairn show --json`，真实数据库随之升到版本 2；只读核对：`schema_version` 为 2、`PRAGMA integrity_check` ok、和备份逐表比过行数，记录 19、项目 2、事件 66、注入 17 都没变。备份留着，删不删由用户定。
- 当时没验证的：真实会话里四种 hook 是不是都记上了时间（后来 cairn/main 在 paddock 里看到了，见“下一步”第 1 条，只差 Claude 的 SessionEnd）；磁盘满、进程被杀时的升级（靠 SQLite 事务，只测了语句失败回滚），这条仍没验证。

## 现在在哪

- **试点已经在跑**（不是“等用户点头”）：
  - Claude Code 的 hook 10-08 装了（`~/.claude/settings.json`：4 个 hook 加一条放行稳定路径 `cairn save` 的规则）。
  - Codex 的 hook 10-10 装了（用户：“给codex也装上吧”，paddock/main 执行；`~/.codex/hooks.json` 4 处，改前备份 `~/.codex/hooks.json.bak-20261010T071731…`）。直接启动的 Codex 要用户到 `/hooks` 里信任，10-10 20:33 看 paddock 里的 `last_seen`，Codex 四种事件都已有时间（17:43–17:48），触发过了。
  - 已采用的项目：owlet、paddock（paddock 10-10 起，和它的全量 HANDOFF 并存试一两周，10-24 前后用户回看）、cairn 自己（10-11 起，同样和本文件并存）。owlet 的 HANDOFF 已减到只留稳定背景，进度靠 cairn 的记录，所以**真实数据库不能随手清**。
  - ranch（corral／派活技能）派出去的 agent 一律带 `CAIRN_DISABLE=1`（ranch R2），cairn 只留给用户直接对话的主控。
- **paddock 已经在用 cairn**：右侧栏第四个标签 Cairn（paddock P5-55，已合并安装）调用 `cairn status --json`、`cairn show --json`、`cairn adopt` 三条，面板开着时每 5 秒读一次（`show` 会顺手收取暂存区）。paddock 的 P5-78 要在面板上显示 `last_seen`；P5-79 要把正文改成记录列表、点开看单条，用 0.3.0 的 `cairn list --json` 和 `cairn show <ID> --json`（都由 paddock 主控做，进度看 paddock 的 HANDOFF）。
- **真实环境装的是 0.3.0**（main 也是 0.3.0），数据库表结构版本 2。
- 现在没有开着的 dev / test agent，没有未清理的 worktree 或分支；`cairn-worktrees/` 下只剩共用编译目录 `.target` 和 `p1-lab`。
- 以下是 10-07 之前各阶段的情况（没变）：
  - **阶段 1（能力实测）已完成**：门槛 A、B、C、F 在 Claude Code 2.1.289 和 Codex 0.160.0 上满足本次实测范围。报告：`docs/调研/第一阶段能力实测.md`；任务与审查记录：`docs/tasks/P1-*.md`。
  - DESIGN 已按实测结论修订（§8.3、§9.1、§10、§14、§15），并新增**暂存区写入设计**（§6.5）：`cairn save` 只写用户私有临时目录里的暂存文件，hook 和用户命令收进数据库。经独立交叉审查两轮复核通过。
  - **阶段 3（接入）已完成**：3a hook 适配（`hook.rs`，`cairn hook claude|codex`）、3b install / uninstall / status（`install.rs`、`status.rs`）均经交叉审查合并；3c 仓库根 `README.md` 已合并。阶段 1 探针已删除。阶段 3 的"建议验证"经用户同意写成验收条件。main 上 102 项测试通过。
  - **阶段 2（核心）已完成**：正式 crate 在 `crates/cairn`（用户定）。已合并：2a 存储（`store.rs`、`store/schema.rs`）、2b 作用域与 Git 事实（`scope.rs`、`facts.rs`）、2c adopt / save / 暂存区收取（`adopt.rs`、`save.rs`、`spool.rs`、`ingest.rs`、`cli.rs`）、2d 注入渲染与 `cairn show`（`render.rs`、`session.rs`）、2e 回合判定（`turn.rs`）、2f 用户命令（`commands.rs`）。全部经交叉审查，main 上 86 项测试通过。
  - 阶段 2 的任务书照用户同意，把实施计划的"建议验证"写成验收条件。
  - 试点前 dry-run 发现并修复：注入与续跑提示里的 save 命令改用稳定路径，与安装写入的权限规则逐字一致（F1，DESIGN §9.2 补注）。
  - **端到端冒烟测试已完成**（`docs/调研/端到端冒烟测试.md`）：真实 Claude Code、Codex 主路径走通（Codex 通过、Claude 条件通过，无已证实代码缺陷）。仓库已公开：https://github.com/firegnu/cairn（暂不加许可证，用户决定）。

## 下一步

**首要（用户 10-12 定）：接续记录记得太频繁，改走旁路模型路线。** 等用户看 `docs/旁路摘要设计草案.md` 并定第 7 节的问题（最要紧的：旁路模型从哪来；什么时候写，主控建议下一个会话开局时写，代价是开局多等 9 到 30 秒；`cairn save` 留不留）。定下来的并进 `docs/DESIGN.md`（要动的条目列在草案第 4 节，含 `AGENTS.md` 隐私一条），再拆实现任务；动 `list` 的输出先看 §7.1。在正式文档改完之前，试点照现在的设计跑。下面第 0 条排在它后面。

0. **和用户详细讨论设计草案**（见“10-11 的会话”的“新方向”一段）：先定“智能”指什么和草案 §1.2 的定位，再逐条过 `docs/记忆系统设计草案.md` 第 12 节的八个问题。谈定的部分并进 `docs/DESIGN.md`（D2、§6.3 末句、§7 受限入口、§13“不做”要动），再拆实现任务；第一期开工前先做一次小实测（第二个 agent 可用的命令在两家的权限和沙箱下走不走得通）。
1. **看试点**（用户定：直接用 paddock 试；10-11 起本仓库自己也采用了，新会话开局先看这里的注入准不准）：在已采用的项目里跑 `cairn status`，看每家“本项目最近触发”那一行（或 `--json` 的 `last_seen`）。10-10 20:33 在 paddock 里看到：Claude 的 SessionStart、UserPromptSubmit、Stop 都有时间，SessionEnd 还没有；Codex 四种事件都有时间（17:43–17:48），说明 Codex 的 hook 在 paddock 里触发过。还没看的：重开后注入的内容准不准。10-24 前后用户回看 paddock 的试点（HANDOFF 瘦不瘦由用户定）。
2. **paddock 还想要、没做的**（见 `docs/调研/与paddock结合.md`，开不开工由用户定）：`show --json`（不带 ID）分节。做的时候按 §7.1 的“只加不改”来，新字段写进 §7.1。

## 悬而未决

- P1-04 实测的遗留物，删不删由用户定：`cairn-worktrees/p1-lab/sidecar/`（约 23 MB，含测试会话记录的副本，都是合成内容）；`p1-lab` 两个仓库里各一份 `*.before-sidecar-20261012` 配置备份；两家工具自己保留的 7 个测试会话的记录。
- 来源的最后一条记录被取代后，注入退回显示它更早的记录（10-12 观察到，见“本次会话”），改不改由用户定。要动先看 D9 和 §8.1，在隔离目录用合成输入复现。
- 端到端冒烟测试已完成（`docs/调研/端到端冒烟测试.md`）：Codex 通过，Claude 条件通过。S1-1：项目规则限制命令时 Claude 可能不确认、续跑后也不确认（cairn 如实记录，不无限续跑）；是否调整注入措辞，试点后由用户决定。
- `~/.local/state/cairn/errors.log` 10-10 有 9 条，全是 claude，来源没查（用户说暂时不用）：8 条是 SessionStart／SessionEnd 的输入解析失败，集中在 17:06–17:16（F2 施工期间，其中三条在 0.1 秒内，像是拿合成输入直接跑了 `cairn hook claude`，F2 记录里没提，没确认）；1 条是 14:58 的 Stop 处理失败，原因日志里看不出。如果解析失败来自真实会话，说明有些 Claude 会话拿不到注入；之后再出现就要查，查时在隔离目录用合成输入复现。
- 冒烟遗留物：`p1-lab/smoke/`；私有临时目录下空的 `cairn-spool/d7de8e374f1b8ca2/`；`p1-lab` 两个仓库里各两个未跟踪的合成文件。删不删由用户定。
- 3c 走查在真实私有临时目录留下空目录 `cairn-spool/7262111286db64b8/`（无文件），删不删由用户定。
- 升级前的数据库备份 `~/.local/state/cairn-backup-20261010-before-v2/`（版本 1，10-10 17:12 的状态），删不删由用户定。退回 0.1.0 的话要连数据库一起换回备份：0.1.0 不认版本 2 的库（hook 会放行、什么都不做），换回后备份之后的记录就没了。
- 对真实配置的改动（`cairn install`／`uninstall`）照旧要用户明确同意。现状：两家都已装；卸掉用 `cairn uninstall --agent claude|codex`。
- DESIGN 在阶段 2 中的修订：收取必须向前推进（9585ea1）；resume/fork 补注入规则与回合起点约定（a5c9141）；无 turn_key 不续跑（9f6f20e）；correct 只以 checkpoint 为目标（afd2212）；取代 / 撤回 / 恢复按 rowid 判定先后、不执行 VACUUM（a520af2）。
- 2b 审查的建议改：空仓库、detached HEAD、脏工作区计数、upstream ref 缺失没有专项测试，可在 2c / 2d 集成时补。
- DESIGN §6.5 已改（9585ea1）：收取必须向前推进，来源优先是尽力而为。
- save 测试因积压回归约需 50 秒。
- 测试里 git 超时 2 秒，并行跑多个测试时可能偶发超时（2e 审查记录），可在 2f 或之后放宽测试用超时。
- 措辞红线只管 cairn 自动生成的文字，模型写的历史正文原样保留（用户确认）。
- 存储层对宽权限、符号链接一律拒绝不自动修复（2a 交叉审查同意的取舍）。
- **实测遗留物，删不删由用户决定**（详细位置见报告 §12）：
  - `cairn-worktrees/p1-lab/` 实验目录；
  - 私有临时目录下的 `cairn-probe-spool-p1-*` 子目录；
  - 三条信任记录：`~/.claude.json` 里 `p1-lab/claude-repo` 的项目条目；`~/.codex/config.toml` 里 `p1-lab/codex-repo` 的项目信任和 SessionStart hook 的信任记录。
- 已知限制（用户决定不挡阶段 2）：Codex 非交互 `exec` 有一次没复述注入内容，原因未查。
- 被委派的 agent 关闭 cairn：已由 ranch 在派活时加 `CAIRN_DISABLE=1` 解决（ranch R2），cairn 自己没有默认关闭的逻辑。
- 远程仓库：用户 2026-10-05 建了 public 仓库 github.com/firegnu/cairn（origin），每次合并后推送；仓库是公开的，推送前确认没有密钥和私人数据（F2 推送前跑过 gitleaks）。
- Saddle `t49-memory-design` 分支上的 T49 文档，"同仓、随 Saddle 发布"和"文件存储"两处已被本仓库 D1、D3 取代；Saddle 那边要不要补说明，由用户决定。
