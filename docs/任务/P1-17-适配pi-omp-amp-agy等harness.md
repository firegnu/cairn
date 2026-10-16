# 任务：cairn 适配 pi、omp、amp、agy 等 harness

2026-10-16，用户让主控记成草稿任务。**草稿，没派。**

## 用户原话

2026-10-16，和 cairn/main（主控）讨论 cairn 对哪些 agent 起作用时：

1. “我理解了，只要我hook再安装在pi，omp等之下，自然而然pi， omp这种也就起作用了”（主控答：不完全是，见下面“现状”）
2. “那就加一个draft task，cairn适配pi，omp，amp，agy等harness”

## 要做的

现在 cairn 只对 Claude Code 和 Codex 起作用。要让 pi、omp、amp、agy 等 harness 也能用上 cairn：开局看到上次停在哪，会话之后有摘要。

## 怎么算做完

用户还没说。

## 现状（主控核对过）

- `docs/DESIGN.md` §1 写着“只支持 Claude Code 和 Codex”；`docs/背景与决策记录.md` 写着“首版只承诺 Claude Code 和 Codex。其他 agent 每接一个，都要单独实测。”
- 只装 hook 不够。cairn 对每家 agent 要做到这几件事：
  1. 开局能把一段文字注入 agent 的上下文（Claude、Codex 用 SessionStart 返回 `additionalContext`）；
  2. 回合开始、回合结束、会话结束时能通知 cairn（UserPromptSubmit、Stop、SessionEnd）；
  3. 能给出会话 ID、工作目录和会话记录文件的路径（旁路模型写摘要要读这个文件，DESIGN §8.8）。
- 每家都要在 cairn 这边配套：`cairn hook <agent>` 的输入适配（现在只认 `claude`、`codex`）；会话记录解析器（DESIGN §8.8，现在只认两家的格式）；`cairn install` / `uninstall` / `status` 写它的配置（现在 `--agent` 只认 `claude`、`codex`）。
- pi、omp、amp、agy 有没有 hook、能力够不够，**主控没查过**。

## 主控的理解（待用户确认）

以下是主控的理解，用户还没确认，不是需求。

- 先做调研：逐家查有没有 hook，事件、输入字段、注入方式、会话记录格式和位置，列出每家能做到上面哪几件、做不到的怎么办。结论交给用户定接哪几家、按什么顺序接。
- 定下要接的，每家先像阶段 1 那样实测（门槛 A 注入、B 回合，见 `docs/实施计划.md`），过了再改 DESIGN、拆实现任务。
- 能力不全的（比如没有开局注入），是退一步只做一部分，还是不接，由用户定。

## 还没定的

- 接哪几家、先后顺序；“等”里面还有没有别的 harness。
- 某家能力不全时怎么办。
- 动工前先改 `docs/DESIGN.md` §1“只支持 Claude Code 和 Codex”那句。
