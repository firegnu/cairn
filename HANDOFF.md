# 交接

2026-10-05，由主控 cairn/main（Claude Code）更新。

## 现在在哪

- **阶段 1（能力实测）已完成**：门槛 A、B、C、F 在 Claude Code 2.1.289 和 Codex 0.160.0 上满足本次实测范围。报告：`docs/调研/第一阶段能力实测.md`；任务与审查记录：`docs/tasks/P1-*.md`。
- DESIGN 已按实测结论修订（§8.3、§9.1、§10、§14、§15），并新增**暂存区写入设计**（§6.5）：`cairn save` 只写用户私有临时目录里的暂存文件，hook 和用户命令收进数据库。经独立交叉审查两轮复核通过。
- 阶段 2 进行中：正式 crate 在 `crates/cairn`（用户定）。2b 作用域与 Git 事实已合并（`scope.rs`、`facts.rs`）。2a 存储由 cairn/dev-store 在分支 `p2a-store` 上做，做完要交叉审查。
- 阶段 2 的任务书照用户同意，把实施计划的"建议验证"写成验收条件。

## 下一步

1. 审查并合并 2a（含交叉审查）。2a 和 2b 都往 `crates/cairn/Cargo.toml` 加了依赖，合并时可能要解决冲突。
2. 之后派 2c（adopt 与 save，按 DESIGN §6.5、§8.2 两段式）；2d、2e 依赖 2a，可以视情况并行。
3. 探针 `tools/cairn-probe` 什么时候删，还没定。

## 悬而未决

- **实测遗留物，删不删由用户决定**（详细位置见报告 §12）：
  - `cairn-worktrees/p1-lab/` 实验目录；
  - 私有临时目录下的 `cairn-probe-spool-p1-*` 子目录；
  - 三条信任记录：`~/.claude.json` 里 `p1-lab/claude-repo` 的项目条目；`~/.codex/config.toml` 里 `p1-lab/codex-repo` 的项目信任和 SessionStart hook 的信任记录。
- 已知限制（用户决定不挡阶段 2）：Codex 非交互 `exec` 有一次没复述注入内容，原因未查。
- 是否默认对被委派 agent / 脚本会话关闭 cairn（`CAIRN_DISABLE=1`），等试点后由用户决定。
- 远程仓库：用户暂不建。
- Saddle `t49-memory-design` 分支上的 T49 文档，"同仓、随 Saddle 发布"和"文件存储"两处已被本仓库 D1、D3 取代；Saddle 那边要不要补说明，由用户决定。
