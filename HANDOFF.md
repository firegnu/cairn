# 交接

2026-10-05，由建立仓库的 Claude Code 会话写下。

## 现在在哪

- 仓库刚刚建立，只有文档，没有代码。只有本地 git，没有远程仓库。
- 设计已经定下来（`docs/DESIGN.md` §2，D1–D12）。用户已指示按这份设计开工，由本仓库的新主控接手。
- 来历与理由见 `docs/背景与决策记录.md`。设计源自 Saddle 的 T49 讨论。

## 下一步

1. **用户开主控**（在 Saddle 或任意终端里执行）：

   ```sh
   corral start cairn/main --cwd /Users/firegnu/Developer/personal_projs/cairn --label role=controller -- claude
   corral attach cairn/main   # 第一次进新目录时需要点信任，点完按 Ctrl-] 断开
   ```

   主控也可以用 Codex。新目录第一次打开时，两种工具都会弹出信任对话框。
2. **主控先读** `AGENTS.md` 中"先读"列出的文档，然后从 `docs/实施计划.md` 的**阶段 1：能力实测**开始。建议先派"探针程序"这个小任务，再派"实测与报告"。实测中有几项需要用户在旁边点信任（Claude 目录信任、Codex 项目与 hook 信任）。
3. 阶段 1 的 A、B、C、F 四项在两种工具上都通过之后，才进入阶段 2 的核心实现。C 项（写入传输）如果有工具做不到，就停下来向用户报告。

## 悬而未决

- `docs/DESIGN.md` §15 的待定问题，大多要靠阶段 1 的实测结论来定。
- 远程仓库：用户暂不建；需要时由用户决定。
- Saddle 仓库 `t49-memory-design` 分支上的 T49 文档里，"同仓、随 Saddle 发布"和"文件存储"两处，已被本仓库的 D1、D3 取代。Saddle 那边要不要补一条说明，由用户决定。
