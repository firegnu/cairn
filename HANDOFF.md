# 交接

2026-10-05，由主控 cairn/main（Claude Code）更新。

## 现在在哪

- 设计已定（`docs/DESIGN.md` §2，D1–D12），来历见 `docs/背景与决策记录.md`。只有本地 git，没有远程仓库。
- 阶段 1（能力实测）进行中：
  - **探针程序已完成并合并**：仓库根是虚拟 Cargo workspace，唯一成员 `tools/cairn-probe`（阶段 1 结束后删除）。用法和输出约定见 `docs/tasks/P1-探针程序.md`。
  - 实测本身还没开始。
- 没有开着的 dev / test agent；没有未清理的 worktree 或分支。

## 下一步

1. 派"实测与报告"任务：按 `docs/实施计划.md` 阶段 1 的 A–H 项，在本机真实 Claude Code 和 Codex 上用 `cairn-probe` 实测，产出 `docs/调研/第一阶段能力实测.md` 并修订 DESIGN §8.3、§9.1、§10、§14、§15。
   - 只用会话级参数或临时目录的项目级配置注入 hooks，不改用户真实配置。
   - 有几项需要用户在旁点信任（Claude 目录信任、Codex 项目与 hook 信任）；实测结束告诉用户如何清除这些信任记录。
2. A、B、C、F 在两种工具上都通过后，才进入阶段 2。C 项（写入传输）有工具做不到，就停下向用户报告。

## 悬而未决

- `docs/DESIGN.md` §15 的待定问题，大多等阶段 1 结论。
- 远程仓库：用户暂不建。
- Saddle `t49-memory-design` 分支的 T49 文档里，"同仓、随 Saddle 发布"和"文件存储"两处已被本仓库 D1、D3 取代；Saddle 那边要不要补说明，由用户决定。
- 正式 `cairn` crate 放在 workspace 的哪个位置（根 package 还是 `crates/cairn`），阶段 2 开工时定。
