# 交接

本文件只记照着执行的事实：真实环境装的是哪一版、怎么退回，机器和环境的长期事实，还没派的草稿，已知没修的问题。只在这些变了时更新。

- 会话经过、进度、待定的事、约定和经验由 cairn 记（接续摘要、项目的账、经验），开局自动注入；这里不重复。现在有没有进行中的活、worktree、测试 agent，去现场查（`git worktree list`、`corral ls`）。
- 设计和理由在 `docs/DESIGN.md`（待定问题在 §15），每件活的经过在 `docs/任务/`（P1-01 之前的在 `docs/tasks/`）和「合并」「收尾」提交里。
- 2026-10-23 之前的版本（各版装的经过、阶段 1–3 的进度、试点经过、下一步、悬而未决）在 git 历史里：`git log -p -- HANDOFF.md`。

## 真实环境安装

- 现装 **0.8.0**（11-11，P1-27 接入 canter）：`cargo install --path crates/cairn` 装到 `~/.cargo/bin/cairn`，稳定路径 `~/.local/share/cairn/bin/cairn` 链到它。数据库在 `~/.local/state/cairn/`，版本 6。canter 的钩子 11-11 装在 `~/.canter/config.json`（4 个 hook，不写 timeout；改前备份 `~/.canter/config.json.bak-20261111T122404…`），`canter doctor` 两项配置检查 ok；用真实 canter 实测还没做。
- 退回 0.7.x：0.7.x 读不了版本 6 的库，要连库换回 `~/.local/state/cairn-backup-0.7.1-20261111-202152/`（版本 5），之后写的会丢。0.7.0 和 0.7.1 之间库结构没变。退回 0.6.x：0.6.x 读不了版本 5 的库，要连库换回 `~/.local/state/cairn-backup-0.6.2-20261017-131451/`（版本 4），之后写的会丢。
- 历次装前的库备份都在 `~/.local/state/cairn-backup-*`（0.7.1、0.7.0、0.6.2、0.6.1、0.6.0、0.5.0、0.4.1、0.4.0、0.3.0，以及 `cairn-backup-20261010-before-v2` 版本 1）。库有 WAL，备份用 `sqlite3 .backup`，不直接复制文件。删不删由用户定。
- 装新版的做法：装前用 `sqlite3 .backup` 备份库并跑完整性检查；装后核对 `cairn --version` 两个路径一致。hook 定义没变就不重装。
- canter 没有 hook 信任审核，改了配置下一轮生效；卸掉用 `cairn uninstall --agent canter`。
- hook：Claude Code 10-08 装在 `~/.claude/settings.json`（4 个 hook，加一条放行稳定路径 `cairn save` 的规则）；Codex 10-10 装在 `~/.codex/hooks.json`（改前备份 `~/.codex/hooks.json.bak-20261010T071731…`），10-16 重装过（`additionalContextLimit` 12000）。直接启动的 Codex 要用户到 `/hooks` 里信任。卸掉用 `cairn uninstall --agent claude|codex`；对真实配置的改动要用户明确同意。

## 机器和环境

- 已采用 cairn 的项目：cairn、paddock、ranch、owlet（pasture 也显示已采用，不纳入）。owlet 的 HANDOFF 已减到只留稳定背景，进度靠 cairn，所以**真实数据库不能随手清**。
- paddock 面板调用 cairn 的公开命令：`status --json`、`show [<ID>] --json`、`list --json`、`adopt`（面板开着时每 5 秒读一次）。动输出前先看 DESIGN §7.1（只加不改）。
- cairn 只认 `CAIRN_DISABLE=1`，不判断谁是主控。ranch 的 corral、corral-dispatch 技能开 agent 时带这个变量；其余方式开的 agent，目录已采用就会用 cairn。
- PATH 里排第一的 `codex` 是 IP Guardian 的包装脚本，查完出口 IP 再调真的 codex。
- 本机 Codex memories 是开着的（`codex features list` 显示 `memories stable true`，和官方文档说的默认关不一致，原因没查）。派出去的 Codex 带 `--disable memories`；cairn 旁路的 `codex exec` 从 0.6.1 起也带上。
- 编译目录：`../cairn-worktrees/.target/<子目录>`，每个工作目录一个（见 AGENTS.md）。
- `origin` = `github.com/firegnu/cairn`（public，暂不加许可证）。推送前确认没有密钥和私人数据（跑 gitleaks）。

## 实测遗留物（删不删由用户定）

- `../cairn-worktrees/p1-lab/`：阶段 1 实验仓库，以及 `sidecar/`（P1-04）、`midturn/`（P1-05）、`budget/`（P1-14）、`smoke/`（冒烟）各目录；两个仓库里的 `*.before-*` 配置备份和未跟踪的合成文件。内容都是合成的。
- 私有临时目录下的 `cairn-probe-spool-p1-*`，空的 `cairn-spool/d7de8e374f1b8ca2/`、`cairn-spool/7262111286db64b8/`。
- 三条信任记录：`~/.claude.json` 里 `p1-lab/claude-repo` 的项目条目；`~/.codex/config.toml` 里 `p1-lab/codex-repo` 的项目信任和 SessionStart hook 的信任记录。
- 两家工具自己保留的测试会话记录（P1-04 七个、P1-05、P1-14 三个）。
- 详细位置见 `docs/调研/第一阶段能力实测.md` §12。

## 还没派的草稿

- `docs/任务/P1-01-扩展成一个完整的记忆系统.md`：只有设计草案 `docs/记忆系统设计草案.md`（§12 八问待定），项目的账、经验已另做掉，剩经验手册、全局记忆。
- `docs/任务/P1-12-show-json区分全文和一行.md`：验收用户还没说；paddock 想要的 `show --json`（不带 ID）分节（`docs/调研/与paddock结合.md`）可以一起看。
- `docs/任务/P1-17-适配pi-omp-amp-agy等harness.md`：先调研各家 hook 能力，用户再定接哪几家。
- `docs/任务/P1-18-并行一段时间后删除HANDOFF.md`：长期记忆的四期。
- `docs/任务/P1-23-记忆系统的后续缺口.md`：条目多了开局只剩标题、记下的对不对没反馈、跨项目经验没地方去。
- `docs/任务/P1-24-记账停了但不报失败.md`：第一步“留痕”已由 P1-25 做掉；剩截断（60 KiB 头尾、工具输出 500 字），等看了 `ledger runs` 的截断统计再定（DESIGN §15）。
- `docs/任务/P1-28-读Codex压缩后的会话记录.md`：Codex 把旧会话记录压成 `.jsonl.zst` 后 cairn 报 NotFound（开局“3 个会话记账没写成”就是这个）；用户选解压了照样读；用户 11-11 说暂时不做，以后再做。
- `docs/任务/P1-29-记账正文超长和标题不合格.md`：paddock 记账 200 次失败 61 次，多是“正文超过 1 KiB”（按字节算）和“标题不合格”；正文怎么改用户还没定，用户说找时间修。
- `docs/任务/P1-30-安装预览只显示hooks.md`：install/uninstall 的预览会打出整份配置（canter 的 api_key 11-11 因此露出）；改成只显示 hooks 的改动。

## 已知没修的（不主动追，碰到再看）

- 摘要会把退出前的状态写成现在时（会话记录停在 `/exit` 之前）。
- 同一目录并排跑两个 agent：现场对比分不清是谁改的。
- 来源的最后一条记录被取代后，注入退回显示它更早的记录（要动先看 D9 和 §8.1，在隔离目录用合成输入复现）。
- 没测成或没查的：P1-06 Codex `/exit` 后后台进程能不能活；F2 升表结构时磁盘满、进程被杀；冒烟 S1-1（0.4.0 起可能已不适用，没复核）；Codex 非交互 `exec` 有一次没复述注入。
- `~/.local/state/cairn/errors.log` 10-10 有 9 条 claude 的（8 条 SessionStart／SessionEnd 输入解析失败，集中在 F2 施工的 17:06–17:16；1 条 14:58 Stop 处理失败），来源没查。再出现就要查，在隔离目录用合成输入复现。
- 测试：save 回归约 50 秒；git 超时 2 秒、1 秒时限的超时测试在并行或高负载下偶发失败，单跑、重跑都过；2b 审查建议补的空仓库、detached HEAD、脏工作区计数、upstream ref 缺失的专项测试没补。
