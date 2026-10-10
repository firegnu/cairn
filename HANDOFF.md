# 交接

2026-10-10，由 cairn/main（主控，Claude Code）更新。同日更早 paddock/main 按用户的话直接在本仓库做了 F2 并改过本文件，那部分留在“同日更早”一节。

## 本次会话（10-10，cairn/main）

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

## 同日更早（10-10，paddock/main 在本仓库做的 F2）

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
  - 已采用的项目：owlet、paddock（paddock 10-10 起，和它的全量 HANDOFF 并存试一两周，10-24 前后用户回看）。owlet 的 HANDOFF 已减到只留稳定背景，进度靠 cairn 的记录，所以**真实数据库不能随手清**。
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

1. **看试点**（用户定：直接用 paddock 试）：在已采用的项目里跑 `cairn status`，看每家“本项目最近触发”那一行（或 `--json` 的 `last_seen`）。10-10 20:33 在 paddock 里看到：Claude 的 SessionStart、UserPromptSubmit、Stop 都有时间，SessionEnd 还没有；Codex 四种事件都有时间（17:43–17:48），说明 Codex 的 hook 在 paddock 里触发过。还没看的：重开后注入的内容准不准。10-24 前后用户回看 paddock 的试点（HANDOFF 瘦不瘦由用户定）。
2. **paddock 还想要、没做的**（见 `docs/调研/与paddock结合.md`，开不开工由用户定）：`show --json`（不带 ID）分节。做的时候按 §7.1 的“只加不改”来，新字段写进 §7.1。

## 悬而未决

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
