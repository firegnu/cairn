# cairn

给终端里 coding agent 用的工作接续记忆工具。名字取自登山路上的石堆路标：前面的人留给后来者看的标记。

- 开一个全新的 agent 会话时（哪怕换了一个工具），开局就能自动看到上次停在哪、现场有哪些可观测的变化、建议的下一步。
- 每个正常结束的回合，agent 都有一次机会，把需要接续的内容通过 `cairn save` 交给 cairn 保存。回答本身不出现任何记忆标记。
- 记录存放在仓库外的本机用户目录里，保存时不提交、不推送。只有用户用 `cairn adopt` 采用的项目才会生效。
- 记录是带来源的历史，不是授权。

**状态**：只有设计，还没有代码。下一步是阶段 1 能力实测，这是开始实现的门槛。

## 文档

| 文档 | 内容 |
|---|---|
| [docs/DESIGN.md](docs/DESIGN.md) | 权威设计 |
| [docs/实施计划.md](docs/实施计划.md) | 各阶段、实施门槛、任务拆分建议 |
| [docs/背景与决策记录.md](docs/背景与决策记录.md) | 来历、用户原话、为什么这样定 |
| [docs/调研/agent-hooks资料.md](docs/调研/agent-hooks资料.md) | Claude Code / Codex hooks 官方资料摘录 |
| [AGENTS.md](AGENTS.md) | 开发规矩与主控分派流程（`CLAUDE.md` 是它的软链接） |
| [HANDOFF.md](HANDOFF.md) | 当前进度与下一步 |

## 首版范围

- 只在本机使用。
- 只支持 Claude Code 和 Codex。
- 用 SQLite 存储，Markdown 用于导出。

核心逻辑和具体 agent 无关：再接一种新 agent，只需要加一个适配器。
