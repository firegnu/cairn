# 交接

2026-10-05，由主控 cairn/main（Claude Code）更新。

## 现在在哪

- **阶段 1（能力实测）已完成**：门槛 A、B、C、F 在 Claude Code 2.1.289 和 Codex 0.160.0 上满足本次实测范围。报告：`docs/调研/第一阶段能力实测.md`；任务与审查记录：`docs/tasks/P1-*.md`。
- DESIGN 已按实测结论修订（§8.3、§9.1、§10、§14、§15），并新增**暂存区写入设计**（§6.5）：`cairn save` 只写用户私有临时目录里的暂存文件，hook 和用户命令收进数据库。经独立交叉审查两轮复核通过。
- 仓库现在只有探针 `tools/cairn-probe`（虚拟 workspace 唯一成员），正式 `cairn` crate 还没开始写。
- 没有开着的 dev / test agent；没有未清理的 worktree 或分支。

## 下一步

1. 开阶段 2（核心）。按 `docs/实施计划.md` 阶段 2 拆任务；地基先串行：2a 存储（含 `spool_ops`、`confirmations.op_id`），与 2b 作用域与事实可以并行。2c 的 save 按 DESIGN §6.5 / §8.2 两段式实现。
2. 开工前先定正式 `cairn` crate 在 workspace 里的位置（根 package 还是 `crates/cairn`），以及探针什么时候删。

## 悬而未决

- **实测遗留物，删不删由用户决定**（详细位置见报告 §12）：
  - `cairn-worktrees/p1-lab/` 实验目录；
  - 私有临时目录下的 `cairn-probe-spool-p1-*` 子目录；
  - 三条信任记录：`~/.claude.json` 里 `p1-lab/claude-repo` 的项目条目；`~/.codex/config.toml` 里 `p1-lab/codex-repo` 的项目信任和 SessionStart hook 的信任记录。
- 已知限制（用户决定不挡阶段 2）：Codex 非交互 `exec` 有一次没复述注入内容，原因未查。
- 是否默认对被委派 agent / 脚本会话关闭 cairn（`CAIRN_DISABLE=1`），等试点后由用户决定。
- 远程仓库：用户暂不建。
- Saddle `t49-memory-design` 分支上的 T49 文档，"同仓、随 Saddle 发布"和"文件存储"两处已被本仓库 D1、D3 取代；Saddle 那边要不要补说明，由用户决定。
