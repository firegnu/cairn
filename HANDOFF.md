# 交接

2026-10-05，由主控 cairn/main（Claude Code）更新。

## 现在在哪

- **阶段 1（能力实测）已完成**：门槛 A、B、C、F 在 Claude Code 2.1.289 和 Codex 0.160.0 上满足本次实测范围。报告：`docs/调研/第一阶段能力实测.md`；任务与审查记录：`docs/tasks/P1-*.md`。
- DESIGN 已按实测结论修订（§8.3、§9.1、§10、§14、§15），并新增**暂存区写入设计**（§6.5）：`cairn save` 只写用户私有临时目录里的暂存文件，hook 和用户命令收进数据库。经独立交叉审查两轮复核通过。
- **阶段 2（核心）已完成**：正式 crate 在 `crates/cairn`（用户定）。已合并：2a 存储（`store.rs`、`store/schema.rs`）、2b 作用域与 Git 事实（`scope.rs`、`facts.rs`）、2c adopt / save / 暂存区收取（`adopt.rs`、`save.rs`、`spool.rs`、`ingest.rs`、`cli.rs`）、2d 注入渲染与 `cairn show`（`render.rs`、`session.rs`）、2e 回合判定（`turn.rs`）、2f 用户命令（`commands.rs`）。全部经交叉审查，main 上 86 项测试通过。
- 阶段 2 的任务书照用户同意，把实施计划的"建议验证"写成验收条件。

## 下一步

1. 进入阶段 3（接入）：3a hook 适配 → 3b install / uninstall / status → 3c 安装方式与 README。开工前先问用户：阶段 3 的"建议验证"要不要当验收条件（阶段 2 的授权不覆盖阶段 3）。
2. 3a 做完、hook 有了自己的夹具测试之后，建议删掉探针 `tools/cairn-probe`（问用户）。

## 悬而未决

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
- 是否默认对被委派 agent / 脚本会话关闭 cairn（`CAIRN_DISABLE=1`），等试点后由用户决定。
- 远程仓库：用户暂不建。
- Saddle `t49-memory-design` 分支上的 T49 文档，"同仓、随 Saddle 发布"和"文件存储"两处已被本仓库 D1、D3 取代；Saddle 那边要不要补说明，由用户决定。
