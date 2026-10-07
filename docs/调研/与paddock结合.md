# 与 paddock 结合

2026-10-07，主控 cairn/main。只读调查两边仓库，没改代码。依据：cairn main `a3d9043`；paddock main `50a5e01`（`README.md`、`HANDOFF.md`、`docs/DESIGN.md` §1–§4、`app/src/` 文件清单）；ranch 的 corral 命令行。

## 1. 结论

能接，而且接法顺：paddock 与外部工具只通过公开命令和 JSON 打交道（paddock DESIGN §3，corral 就是这样用的）；cairn 是独立的命令行，已有 JSON 输出。所以由 paddock 调用 `cairn` 命令，两边都不在 Cargo 里引用对方，cairn 不需要知道 paddock 存在，符合 cairn"独立运行"的规矩。

建议顺序：先完成 cairn 阶段 4 试点 → cairn 把 JSON 输出定成公开约定（必要时补 `list --json`）→ paddock 主控在 paddock 自己的流程里做下面第 1、2 项。第 3–5 项用一段时间再定。

## 2. 两边能对上的地方

| paddock 现有 | cairn 现有 |
|---|---|
| Agents 列表，每个 agent 有工作目录（worktree） | 工作线 = `git rev-parse --show-toplevel`（DESIGN §5），按目录对得上 |
| 右侧栏 Changes／Browser／Kanban 三个面板 | `cairn show --json`：下次注入的内容（`text`、`record_ids`、`omitted_sources`） |
| Kanban 与提醒里的"Needs you"（`待用户：` 约定） | 记录正文的 `## 待用户决定` 一节（DESIGN §6.3） |
| New Agent 对话框，经 `corral start` 开 agent，`corral start` 支持 `--env` | `CAIRN_DISABLE=1` 让该会话的 hook 放行、不注入、不保存 |
| Settings、诊断页 | `cairn status --json`：接入状态、项目是否采用、暂存区积压 |

## 3. 可做的接法（由轻到重）

1. **右侧栏"接续"面板（只读，最值得做）**：选中 agent 后，在它的工作目录跑 `cairn show --json`，显示停点、下一步、待用户决定。Kanban 回答"任务走到哪"，这个回答"这条线上次停在哪"。
2. **New Agent 加"关闭 cairn"选项**：勾上就 `corral start --env CAIRN_DISABLE=1`。cairn 悬而未决的"被委派 agent / 脚本会话是否默认关闭 cairn"因此可以按 agent 单独选，不必一刀切。
3. **待用户决定接进 paddock 的提醒**：记录里的 `## 待用户决定` 作为 Needs you 的一种来源。
4. **设置／诊断页显示 cairn 状态，加"在这个仓库启用"按钮**：状态取自 `cairn status --json`；按钮跑 `cairn adopt`。这是用户在界面上主动点的命令，符合"除 save 外，其他命令只在用户要求时执行"（DESIGN §7）。
5. **（可选）给 paddock 左侧栏活动格子图当数据源**：paddock 那边数据来源未定（其 HANDOFF"下一步"第 4 条）；cairn 每次保存都有时间和工作线，是现成事实，不用 paddock 新增记录，也就不碰用户砍掉的遥测。前提是 cairn 已在用。

## 4. 缺口

- **JSON 不够、也没承诺稳定**：
  - `cairn list` 只有文字输出，没有 `--json`。
  - `cairn show --json`（不带 ID）只给整段文字和记录 ID。要分节显示，得再对每条跑 `cairn show <ID> --json`，再按 `##` 标题拆。
  - 这些 JSON 格式还没在 DESIGN 里写成公开约定。

  paddock 要长期依赖，应先在 cairn DESIGN §7 写明哪些 JSON 输出是公开约定、怎么演进；需要时补 `list --json`。这些是 cairn 的任务。
- **agent 与记录不是一一对应**：cairn 的来源是 `claude:<session_id>` / `codex:<session_id>`，corral 不暴露 session_id，只能按工作目录对应；同一个 worktree 里的几个 agent 看到同一条线。对"接续"来说正合适。
- **先后**：cairn 还没在真实环境试点。先试点，看 S1-1（项目规则限制命令时 Claude 不保存）影响多大，再让 paddock 接，免得界面显示空的或不可靠的内容。paddock 自己的队列里还排着活动格子图和 agent 暂停。

## 5. 分工

- cairn 侧（本仓库流程）：JSON 公开约定、`list --json`。
- paddock 侧（paddock 主控按 paddock 流程）：面板、New Agent 选项、提醒、状态页。cairn 主控不改 paddock 仓库；需要时把本文作为需求交给用户 / paddock 主控。
- 开发测试仍守 cairn 的隐私规矩：不读用户真实 cairn 数据库，用临时 `HOME` / `XDG_STATE_HOME`。
