# agent 记忆系统现状：长期记忆、RSI 与跨 agent 共享

调研日期：**2026-10-11**。这是公开资料调查，不是安装评测，也不是 cairn 的设计方案。

**总览（本轮综合判断）**：这个方向已经有可用产品，发展重点正从“跨会话存下来”转向“选什么、如何修订、如何证明经验有用”。原生 coding agent 已有自动记忆，独立项目则分别走人工可读文件、结构化观察、任务图、检索服务、时态知识图谱等路线；后台提炼和整理也已产品化。不过，记住事实、从经验改变做法、递归改进自身能力，仍是不同层次，不能用“有记忆”证明“已经实现 RSI”。跨工具共享已有 CLI/MCP 产品，跨 agent 的可信更新、语义冲突和污染传播仍没有统一答案。这个判断来自下文样本，而非全行业完成度排名。[Codex 官方记忆文档][codex]、[Engram][engram]、[Letta MemFS][memfs]、[Dreams][dreams]、[记忆综述][survey]（均查阅 2026-10-11）。

## 阅读口径与边界

- **原文事实**：打开官方文档、项目仓库、发行记录或作者论文页面所见。厂商声称有效，意味着“厂商这样报告”，不意味着本轮复现成功。论文只读摘要的地方明确写“摘要”。
- **推测／判断**：由这些材料推导，显式标记。**未知**：本轮未找到直接依据或未实测，不等于功能不存在。
- 搜索结果和第三方转述只用于找原文，报告不以它们作为关键结论的依据。资料以当前文档及近一年发布为主；早期奠基工作保留年份。
- 仓库 `main`、滚动文档与发行版本分别看待；下列版本号仅证明近期发行活动，不保证该版本包含当前 `main` 的全部能力。外部产品结论不取材于本机 agent 记忆、真实会话、数据库或配置。
- 为避免重名混淆，本文 **Engram 专指 `Gentleman-Programming/engram`**；OpenMemory 专指 Mem0 的项目。它们不是所有同名论文或产品的统称。

## 一、agent 的记忆：现在存什么、怎么用

### 1.1 不只是“上次停在哪”

**原文事实**：2025 年 12 月的综述《Memory in the Age of AI Agents》按形式区分 token、参数和潜在表示记忆，按功能区分事实、经验和工作记忆，并把形成、演化、检索作为动态过程；摘要明确指出，只有“长／短期”分类不足以解释现有系统。[论文摘要][survey]（查阅 2026-10-11）。

**本报告的比较口径**：工作记忆回答“正在做什么”，事实记忆回答“已知什么”，经验／程序记忆回答“以后怎么做”。后三者可以共存于 Markdown 或数据库；存储介质本身不能证明它是哪种记忆。本文重点是终端 agent 能使用的外部持久记忆，不展开模型训练阶段的参数记忆。

### 1.2 原生 coding agent

#### Claude Code：指令文件与自动记忆并行

**原文事实**：自动记忆目前分 `user`、`feedback`、`project`、`reference` 四类；Claude 在会话中判断哪些内容值得写入。每仓库一个本机目录，worktree 共享，采用 `MEMORY.md` 索引加主题文件；开局只注入索引前 200 行或 25KB 中先到的限制，细节按需读取。文件持续保留，直到用户或 Claude 编辑、删除；索引接近限制时会提示整理过时项。可以用 `/memory` 查看编辑。文档未把自动记忆描述为强制执行的配置。[官方记忆文档][claude]（查阅 2026-10-11）。

**更新与共享边界**：当前文档已支持在特定文件组合下自动读取 `AGENTS.md`；这让共享项目指令更方便，但自动记忆仍不自动跨机器或云环境同步。旧背景里的“只有 Claude 能用”应理解为**原生自动消费机制的边界**，不能推成“其他程序无法读取这些 Markdown”。后一句是文件格式带来的推断，不是跨厂商自动同步承诺。[官方记忆文档][claude]（查阅 2026-10-11）。

**原文事实／未知**：自定义 subagent 可以配置 `user`、`project` 或 `local` 范围的持久记忆，帮助同一类 subagent 跨次运行积累经验；范围选择并不意味着所有不同 agent 默认共享同一个脑。Anthropic 提供原生文档与配置入口，但本轮未核实该能力的独立使用规模；对自动写错内容的判真、跨记忆回滚和抗投毒效果，也未找到可据此承诺的保证。[subagent 官方文档][claude-sub]（查阅 2026-10-11）。

#### Codex：后台从旧会话提炼，再整合为本地记忆

**原文事实**：OpenAI 当前文档明确支持本地 Codex memories，默认关闭。启用后，从符合条件的旧会话生成摘要、持久条目和支持证据，保存在 Codex home 的 `memories/` 下；跳过活跃或太短的会话，等待足够空闲后后台更新。配置分别提供逐会话提炼模型和全局整合模型，以及使用／生成记忆的控制；`/memories` 可控制当前聊天是否读取及贡献记忆。[OpenAI 官方 Memories][codex]（查阅 2026-10-11）。

**原文事实／未知**：文档承诺对生成字段脱敏，也可排除使用 MCP／网络等外部上下文的会话；这些是输入控制，不能据此推定记忆已防投毒。IDE 使用连接主机的本地库；ChatGPT web 的记忆与本地 Codex 库分开。文件应视作生成状态，强制团队约定仍放 `AGENTS.md`。具体检索排序、逐条冲突裁决和自动遗忘算法在该页面没有完整说明；不能断言“只有 AGENTS.md、没有自动记忆”，也不能断言与 Claude 自动互通。[OpenAI 官方 Memories][codex]（查阅 2026-10-11）。

#### Gemini CLI：分层 Markdown，区分共享与私人笔记

**原文事实**：当前文档描述 agent 直接通过文件工具保存持久事实、偏好和项目详情：共享约定写仓库 `GEMINI.md`，私人项目笔记写独立的项目私有记忆目录，跨项目偏好写全局文件；以后由分层上下文机制加载。`/memory show` 可以检查已加载内容。它是 Google 原生 CLI 的功能，不需要独立向量服务。[Memory files][gemini]、[管理指南][gemini-guide]（查阅 2026-10-11）。

**判断／未知**：文件模式便于人工纠错和借 Git 分享仓库约定；但本轮所读指南未建立自动语义冲突合并、逐条 TTL 或抗污染保证，也没有证明私有笔记会自动共享给 Claude／Codex。使用文件工具不等于已具备完整生命周期治理；本轮没有核实原生记忆的实际使用量。

### 1.3 专门项目：六条有代表性的路线

#### Engram：agent 精选观察，本地 SQLite 为主

**存与写（原文）**：Go 程序，SQLite + FTS5；存结构化观察、会话摘要，也有保存用户 prompt 的能力。agent 通过 `mem_save` 主动记录决策、修复、发现等，内容建议包含 What／Why／Where／Learned。它明确反对无差别保存全部工具调用。入口包含 CLI、HTTP、MCP 和薄插件。[项目 README][engram]、[架构心智模型][engram-model]（查阅 2026-10-11）。

**读与维护（原文）**：先 `mem_context` 或搜索摘要，再按需查时间线与完整观察；FTS 可按项目、类型、范围过滤。同一 `project + scope + topic_key` 可更新原观察；另有精确去重、按 ID 更正、软／硬删除、版本历史、到期复查提示及置顶。冲突候选交由 agent 判定，检索可显示取代／冲突关系；不同 agent 的冲突判断可并存，文档把跨判断者调和留在后续阶段。[完整工具与生命周期文档][engram-docs]（查阅 2026-10-11）。

**污染与共享（原文／判断）**：有 `<private>` 脱敏、修改时的项目归属断言，以及可选 Git chunks／云同步；本地 SQLite 仍为权威，云端另用 Postgres。归属断言防写错项目，并非认证；脱敏防部分泄露，并非判真。当前功能已明显超出早期“单二进制加 MCP”的概括。本轮未找到证明其能可靠阻止恶意经验写入的评测。[完整文档][engram-docs]、[架构心智模型][engram-model]（查阅 2026-10-11）。

#### beads：任务图为核心，另保存持久项目事实

**存、写、读（原文）**：原 `steveyegge/beads` 当前跳转到 `gastownhall/beads`。它用 Dolt 存任务及依赖图，agent 通过 CLI 建任务、认领、更新、关闭；`bd remember` 存项目事实，`bd prime` 输出工作流和记忆，`bd ready` 选择未被阻塞的任务。`bd setup` 已列出 Claude、Codex 等接入，组合 hooks、skill、指令文件。[当前 README][beads]（查阅 2026-10-11）。

**维护与共享（原文／判断）**：Dolt 提供版本、分支与合并；旧已关闭任务可压缩，并有清理命令。数据库合并、唯一 ID 和原子认领解决一部分并发问题，但不证明两条经验在语义上不冲突；本轮没找到自动判定事实真假或拦截记忆投毒的保证。[当前 README][beads]（查阅 2026-10-11）。

**版本差异须保留**：当前 README 把 embedded 描述为默认模式，而 v1.3.1 发行说明说 v1.3.0 起默认是 proxied-server。报告因此只确认“Dolt 支撑任务／记忆、可跨 agent 使用”，不裁定用户安装后一定是哪种默认拓扑。该发行还修复并发关闭 blocker 后任务未进入 ready 的问题，是并发工作流真实维护的迹象，不是无故障证明。[README][beads]、[v1.3.1 发行说明][beads-release]（查阅 2026-10-11）。

#### Claude-Mem：hooks 采集，另一个模型压缩，再逐层召回

**存与写（原文）**：通过生命周期 hooks 捕获会话中的工具观察，由 worker 生成语义摘要；SQLite 保存会话、观察、摘要，检索架构另有 Chroma 向量库。它与 Engram 的主要区别是自动观察和提炼，而不是主要依赖执行任务的 agent 自觉精选。当前仓库也列出 Codex、Gemini 等适配，不应仅凭名字归类为 Claude 独占。[README][cmem]、[架构文档][cmem-arch]（查阅 2026-10-11）。

**读、维护与污染（原文／未知）**：开局注入上下文；搜索采用“索引 → 时间线 → 完整观察”的递进方式。提供 `<private>` 排除及注入配置，但本轮未查明统一的语义纠错、自动淘汰及污染审查机制。跨来源开局注入有单独开关，默认只含当前 harness；“支持多个工具”不意味着默认把全部工具的记忆混合注入。[README][cmem]、[配置文档][cmem-config]（查阅 2026-10-11）。

**判断**：捕获更完整减少漏记，也把原始观察、提炼成本和隐私处理带进系统；不能只比较安装命令数。这是架构取舍，不是本轮性能实测结论。

#### Mem0 / OpenMemory：独立记忆服务，当前 OSS 与云端需分开看

**存与写（原文）**：SDK/API 的 `add` 接收消息，由 LLM 提炼事实、偏好、决策；可用用户、agent、应用、运行 ID 定范围。当前流程是单次提炼后 **ADD-only**，`infer=False` 可保留原输入。OSS 运行于自选基础设施及向量存储，当前检索组合语义、BM25 和实体信号；图存储集成已从当前 OSS 移到 Platform，不能沿用旧版“开源版可直接配图记忆”的描述。[Add 文档][mem0-add]、[新算法迁移指南][mem0-migration]（查阅 2026-10-11）。

**更新与淘汰（原文）**：ADD-only 不等于整个产品不维护旧记录。Platform 的 Dream 在加入记忆时标记取代、合并重复，原记录保留；默认检索仍可能带上已取代项，需 `latest_only` 取当前项。另有可选后台 Synthesis，生成带来源链接的高层模式，但仅处理只带 `user_id` 的记录，附带 agent／run／app ID 的不参加该合成。不要把这套云功能反推成 OSS 已实现。[Dream 文档][mem0-dream]（查阅 2026-10-11）。

**读、纠错与接入（原文）**：调用查询接口检索，可筛选和限制返回量；有按 ID 修改、删除及过期日期。托管 MCP 提供增查改删，文档明确列出 Claude Code、Codex 等；2025 年的 OpenMemory MCP 则是本机共享入口，目标是让兼容客户端使用同一份记忆。两者不能混为“所有处理都离线”：自托管存储是否仍调用远程模型，取决于所配 provider。[Search][mem0-search]、[Update][mem0-update]、[Delete][mem0-delete]、[Add][mem0-add]、[MCP][mem0-mcp]、[OpenMemory 官方介绍，2025][openmemory]（查阅 2026-10-11）。

**未知**：本轮未验证抽取错误率、恶意指令过滤及跨客户端权限隔离的实际效果；身份过滤字段不能单独作为多租户安全保证。当前 Mem0 的活跃开发也不能证明 2025 年 OpenMemory 的所有旧安装说明仍适用于最新版。

#### Graphiti / Zep：把变化中的事实连成带时间的图

**存、写、读（原文）**：Graphiti 由 Zep 维护，存原始 episode、实体、关系和来源；关系有有效时间，旧事实失效后保留历史。应用或 agent 提交 episode，模型抽取、整合图；查询结合向量、关键词和图遍历。开源 Graphiti 自备图后端，如 Neo4j／FalkorDB；Zep 托管产品的专有图引擎与治理能力不能自动算作开源版功能。[Graphiti README][graphiti]（查阅 2026-10-11）。

**维护、接入与风险（原文／判断）**：MCP 支持添加 episode、查节点／事实、按时间过滤、删除边和 episode，`group_id` 用于组织和筛选。来源追踪及失效区间可解释“何时、为何换了说法”；但如果抽取把假信息当成新事实，时间戳并不能纠正它。本轮未核实抗投毒保证，也没有据 `group_id` 推定服务器自动提供完整授权隔离。[MCP 工具文档][graphiti-mcp]（查阅 2026-10-11）。

#### Letta / Letta Code：持久 agent 状态，Git 管理记忆与技能

**当前路线（原文）**：Letta Code 是带本地 CLI 的持久 agent harness，支持本地或云后端；当前主文档描述 MemFS：agent 拥有自己的 Git 记忆仓库，投射成工作机器上的真实 checkout。根目录记忆每轮进入系统提示，带索引的子目录按需读；默认不含向量索引。普通文件工具即可读写，提交提供历史，云备份时再同步。[Letta Code README][letta-code]、[MemFS][memfs]（查阅 2026-10-11）。

**写入和维护（原文）**：agent 学到持久信息时可更新，也可显式 `/remember`；`/doctor` 审查组织、重复与上下文用量。Dreaming 在完成若干步或压缩时由后台 subagent 整理；“agent 审查后应用”仍是另一个模型审查，不是请求用户批准。大型重组先备份，记忆和 agent 自有 skills 都受版本管理。[记忆与 dreaming 指南][letta-memory]、[MemFS][memfs]（查阅 2026-10-11）。

**边界（原文／未知）**：旧 SDK 仍有多个 agent 挂同一个 memory block 的接口；当前指南建议迁移到云端共享 Git 仓库。它不是给任意 Claude／Codex 会话装上就自动共用原生记忆的通用插件。Git 可以追溯更改，不会判定学习是否正确；本轮未核实自动回滚、恶意经验过滤的效果。[共享记忆指南][letta-shared]（查阅 2026-10-11）。

### 1.4 维护活跃度与使用证据

以下是直接读取 GitHub releases API 的 `published_at` 并打开对应发行页所得，日期按 UTC。**活跃是事实；成熟度是有限判断**：发行、修复、集成和公开问题说明项目不只是一篇概念文章，不能换算为生产用户数、长期可靠性或安全认证。

| 项目／维护主体 | 本轮所见近期发行 | 能支持什么；不能支持什么 |
| --- | --- | --- |
| Engram／Gentleman-Programming | v3.3.2，2026-10-10。[发行页][engram-release]（查阅 2026-10-11） | 有当前发行和完整多工具接入文档；独立使用规模未知。 |
| beads／gastownhall（原 steveyegge） | v1.3.1，2026-09-30。[发行页][beads-release]（查阅 2026-10-11） | 并发、备份、迁移问题持续修复；默认拓扑存在上述文档差异。 |
| Claude-Mem／thedotmack | v13.35.0，2026-10-09。[发行页][cmem-release]（查阅 2026-10-11） | 有稳定发行与多 harness 适配；支持列表不是本轮互通实测。 |
| Mem0／mem0ai | Node SDK ts-v3.3.1，2026-09-25。[发行页][mem0-release]（查阅 2026-10-11） | SDK 与商业服务并行维护；不能把 Node 版本号套给 Python SDK 或 OpenMemory。 |
| Graphiti／getzep | v0.30.2，2026-09-08。[发行页][graphiti-release]（查阅 2026-10-11） | 有图后端相关修复；不能用 Zep 商业承诺替代 Graphiti 部署验证。 |
| Letta Code／letta-ai | v0.34.10，2026-10-09。[发行页][letta-release]（查阅 2026-10-11） | CLI 与现行记忆体系持续维护；本轮未验证平台迁移与长期效果。 |

**实际使用迹象的强度**：本轮看到发行修复、集成说明与公开仓库活动，但没有独立核实六个项目的活跃部署数或长期用户收益。因此不以星数作“成熟可靠”的证据，也不把项目自述的 benchmark 增益排成统一榜单。

## 二、RSI：从记经验到递归改进，走到了哪一步

### 2.1 这个缩写在本领域通常指什么

**原文事实**：AI 研究里的 RSI 通常展开为 **Recursive Self-Improvement，递归自我改进**。Sakana AI 的 RSI Lab 用它描述系统参与改进自己的算法、代码乃至底层架构，使后续改进能力继续提升；其文章同时承认分布漂移、通过 benchmark 却不能部署、钻约束空子等失败模式。[RSI Lab 原文][rsi]（查阅 2026-10-11）。

**另一种当前用法**：2026 年 AREX 论文也称自己是 RSI agent，但其摘要描述的是内层收集研究证据、外层逐项验证答案，再定向继续研究；这包含“递归改进当前解答”的含义，不能仅凭名字认定它在跨会话改写自身学习机制。[AREX 摘要][arex]（查阅 2026-10-11）。

**本轮判断**：在所查 AI 文献中，没有发现应优先采用的另一种 RSI 英文展开；歧义主要在“自我改进”究竟改答案、上下文、策略、代码，还是模型。主控所说“从经历沉淀经验，反过来改变以后的做法”对应经验学习／上下文适应，是其中一层，尚不是用户已确认的 RSI 定义。它也不是“强化学习”的另一个缩写。

### 2.2 已有做法形成了几种不同闭环

| 层次 | 写回什么、下次怎样用 | 代表与证据边界 |
| --- | --- | --- |
| 文字反思 | 任务反馈 → 反思文本 → 后续尝试读反思 | **Reflexion（2023）**不更新模型权重，把语言反馈放入 episodic buffer。这里引用早期论文解释方法来源，不用其旧模型成绩代表 2026 年能力。[作者摘要][reflexion]（查阅 2026-10-11） |
| 演化经验手册 | 执行 → 反思 → 逐项整理策略，保留细节 | **ACE（2025，2026 年修订，页面标注 ICLR 2026）**把上下文作为不断演化的 playbook，采用生成、反思、整理过程和结构化增量更新，针对反复重写导致细节丢失的问题。摘要报告任务集上的收益；不是任意仓库长期改进保证。[作者摘要与版本记录][ace]（查阅 2026-10-11） |
| 后台记忆整理 | 多段经历 → 合并／取代／高层模式 → 以后检索 | **Codex、Letta dreaming、Mem0 Dream**已是文档化功能，写入时机、作用域和审批方式各异。它们改善的是未来可用上下文，是否因此提高任务成功率仍需分别评估。[Codex][codex]、[Letta][letta-memory]、[Mem0][mem0-dream]（查阅 2026-10-11） |
| 修改自身程序 | 提出代码修改 → 运行任务评测 → 保留有用 agent 变体 | **Darwin Gödel Machine（2025）**维护不断演化的 agent 变体，以编程 benchmark 验证自改代码。它更接近递归改进自身能力，超出了“多记一些用户偏好”。结果来自作者实验，不是开放世界里无限持续提升的证明。[Sakana 作者工程文章][dgm]（查阅 2026-10-11） |

**产品化例子：Anthropic Managed Agents Dreams（原文）**。Dreaming 当前仍标注 research preview：任务读取一个既有 memory store 和 1–100 个历史 session，生成另一份重组后的 store，合并重复、替换过时项、抽出新洞见；输入不改，使用方可检查输出后采用或丢弃。它是托管 API 的独立能力，不能当作 Claude Code 的本机自动记忆默认运行了同样流程。[Dreams 官方文档][dreams]（查阅 2026-10-11）。

**本轮判断**：可审阅的新 store、版本化文件和经验条目，使“学习产物”变得可见；但可见、可撤销与学习有效是不同证据。模型自评写了一条更漂亮的经验，不足以证明下次少犯错；多 agent 都重复一条经验，也可能只是互相抄写。

### 2.3 论文和工程文章带来的具体进展

- **组织方式从固定条目走向动态关联**：A-MEM（2025）的作者摘要描述带关键词、标签、上下文的笔记；加入新记忆时建立关联，并可更新旧记忆的表征。它把“如何组织和演化记忆”作为模型可参与的工作，而不止一次 embedding 加 top-k。该早期研究用于解释方法分支，本轮没有验证其终端集成和现行维护状态。[A-MEM 摘要][amem]（查阅 2026-10-11）。
- **评测开始问“是否学会环境经验”**：LongMemEval-V2（2026）覆盖静态状态、动态变化、工作流、环境陷阱及问题前提；作者比较检索知识池与 coding agent 主动查文件，后者更准确但延迟更高。这里读取的是摘要，结论仅适用于其 web 环境基准，不能推成真实编程仓库收益。[LME-V2 摘要][lme2]（查阅 2026-10-11）。
- **持久化不等于长期任务就能顺利完成**：Anthropic 2025-11 的长时 agent 工程文用初始化环境、进度文件、Git 历史和逐会话增量工作承接任务；2025-09 的 context engineering 文区分压缩、结构化笔记与子 agent。它们说明外部记忆必须进入工作流程，不能只建立存储接口。这更新了背景记录 §4 的原始参考，但不改变其“偏工作接续”的定位。[长时 harness][harness]、[context engineering][context]（查阅 2026-10-11）。

## 三、跨 agent 的记忆共享：已经能共享什么

### 3.1 不同厂商终端工具之间

**本轮综合判断**：已查到的共享通常靠一个共同的外部入口，而不是互相解析对方的原生私有记忆。以下是接入机制比较，均不代表本轮端到端实测。

| 共享方式 | 已查到的具体做法 | 仍需区分的边界 |
| --- | --- | --- |
| 共同 CLI／本地库 | Engram 让不同客户端读写同一本地 SQLite；beads 让 agent 操作同一任务／事实库。[Engram 架构][engram-model]、[beads][beads]（查阅 2026-10-11） | 都能执行命令，还需要约定什么时候存、什么时候查，以及项目身份如何一致。 |
| 共用 MCP 服务 | Mem0 MCP、Graphiti MCP 公开读写与查询工具。[Mem0][mem0-mcp]、[Graphiti][graphiti-mcp]（查阅 2026-10-11） | MCP 提供调用接口；谁可写、能读哪个用户／项目、是否自动调用，需要服务和客户端规则落实。 |
| 多 harness 捕获后汇聚 | Claude-Mem 提供跨来源开局注入开关。[配置][cmem-config]（查阅 2026-10-11） | 有共同存储不代表默认读取所有来源，也不代表不同来源可信度相同。 |
| 共同指令文件 | Claude Code 当前在条件满足时读取 `AGENTS.md`；Codex 官方建议把团队必需规则留在该文件。[Claude][claude]、[Codex][codex]（查阅 2026-10-11） | 共享静态约定与共享自动提炼的经历是两件事。相同文件名也不保证完全相同的加载优先级。 |

**对旧背景的校正**：Claude 自动记忆、本轮所查 beads、Engram 都已比早期概述更复杂。“agent 主动保存加开局注入”仍是一条路线，但不能概括 Codex 的后台处理、Claude-Mem 的观察模型或图检索。cairn 当前 D2/D12 限定工作接续、且不读 transcript；这些外部系统中有的会提炼历史会话，数据来源边界不同。此处只是现状对照，不提出修改 cairn 的建议。[本地设计 §2](../DESIGN.md#2-已定决定)、[背景 §4](../背景与决策记录.md#4-参考过的外部做法)（本轮只读）；外部事实依据见各项目小节。

### 3.2 同一多 agent 系统内部

**原文事实：共享文件仓库**。Letta 当前共享记忆属于组织拥有的云端 Git 仓库；每个 agent 挂载后得到路径／顶层文件提示，在自己的 checkout 里读写。写者 commit/push，其他 agent pull；这不是瞬时广播，也不是每个 agent 的个人 MemFS 自动合并。旧共享 memory block API 仍支持多个 agent 绑定同一块，但官方现在建议共享仓库。[Letta 共享记忆][letta-shared]（查阅 2026-10-11）。

**原文事实：框架提供作用域，应用定义学习逻辑**。LangGraph 区分按 thread 持久化的工作状态与跨 thread 的 Store；长期项放自定义 namespace，可采用 JSON 文档、内容过滤与语义检索。写入可以在当前执行链路中，也可后台发生；何时删改、如何合并及让哪些 agent 使用，由应用构建。它是 LangChain 的框架能力，不能直接等同于开箱即用的跨厂商终端记忆产品。本轮只读概念文档，未核实其记忆组件的独立部署量、版本和安全效果。[LangChain 官方 memory overview][langgraph]（查阅 2026-10-11）。

**本轮判断：共享至少有四个不同问题**：数据能否到达另一 agent；是否有权使用；冲突由谁判定；经验是否适用于另一任务／模型。Git、SQL 或 MCP 分别解决部分运输、持久化和调用问题，不能由此推出后面三个问题已解决。比如 Engram 将不同判断者的冲突意见保留并列，Graphiti 用有效时间表达事实变更，Letta 用仓库提交表达更新；这些是不同知识治理模型，而非可随意互换的接口。[Engram 冲突工具][engram-docs]、[Graphiti][graphiti]、[Letta][letta-shared]（查阅 2026-10-11）。

## 四、仍未解决好的问题，以及真正的分歧

这里的“公认”限定为**本轮综述、论文和多个项目文档反复出现的问题**，不声称已做社区共识调查。

1. **什么值得留下、何时复核**。全收集再提炼减少漏记，主动精选减少噪声；两者都可能错。Codex 延后处理空闲会话，Claude-Mem 捕获观察，Engram 要求 agent 主动保存，体现的是信息覆盖与成本、时效之间的取舍。没有一个存储后端能替代这个决策。[Codex][codex]、[Claude-Mem 架构][cmem-arch]、[Engram][engram-model]（查阅 2026-10-11；末句为判断）。

2. **“新”是不是就比“旧”正确**。Mem0 的取代、Graphiti 的时态失效与 Engram 的冲突标注各有用途；“同一事实发生变化”与“新输入本来就错了”却不能靠时间排序区分。本轮未查到能够普遍保证正确裁决的方案。[Mem0 Dream][mem0-dream]、[Graphiti][graphiti]、[Engram][engram-docs]（查阅 2026-10-11；区别为判断）。

3. **更新是否会抹掉证据，删除是否真正传播**。反复压缩可能损失细节，ACE 明确把 context collapse 当作问题；Dreams 生成新 store 保留输入，Git 记忆保留提交，时态图保留历史，是不同追溯办法。保留更多历史与遗忘、撤回、隐私删除也有张力。本轮没验证跨所有副本与衍生摘要的端到端删除。[ACE][ace]、[Dreams][dreams]、[MemFS][memfs]（查阅 2026-10-11；删除边界为未知）。

4. **找得到不等于用得对，更不等于持续变强**。LME-V2 已把经验和环境陷阱纳入评测，同时显示准确率与检索延迟的取舍；DGM 则依赖任务评测来筛选自改程序。二者衡量的不是同一个能力。记忆问答成绩、任务完成率、成本及长期避免重犯错误不能混成一个“记忆准确率”。[LME-V2][lme2]、[DGM][dgm]（查阅 2026-10-11；指标区分为判断）。

5. **记忆污染会跨会话延续，共享会扩大传播范围**。2026 年《From Untrusted Input to Trusted Memory》摘要研究多种写入通道，指出积极写入／检索的 agent 更易受攻击，既有 prompt injection 防御未充分覆盖记忆污染。脱敏、来源记录、Git 回滚分别有价值，但不能单独证明恶意指令不会被沉淀成未来的规则。[作者摘要][poison]（查阅 2026-10-11；防护边界为综合判断）。

6. **共享的是事实、个人偏好，还是有约束力的规则**。让 agent 自改记忆／skills 与人维护必需规则，授权边界不同；本地私有、项目共享、组织共享也不是同一范围。Letta 让持久 agent 管理记忆与技能，Codex 官方则提醒必需团队规则放入版本化指令。MCP 并不消除这个治理选择。[Letta Code][letta-code]、[Codex][codex]（查阅 2026-10-11；治理比较为判断）。

这些分歧可以概括为下表。它是对本轮样本的归纳，不是产品优劣榜：

| 分歧 | 一侧 | 另一侧 | 决定的实际问题 |
| --- | --- | --- | --- |
| 谁写 | 执行任务的 agent 精选 | hooks 收集后由模型提炼 | 漏记、成本、隐私暴露和反馈延迟 |
| 存什么 | 文件／可编辑笔记 | 结构化观察、任务图、时态图 | 人工可审阅性与查询、关系表达能力 |
| 怎么读 | 小索引常驻，agent 按需查 | 预先检索、排序、注入 | 上下文预算、召回遗漏和工具耗时 |
| 怎么纠错 | 原地更新／整理 | 保留历史、标记取代／冲突 | 当前答案简洁性与证据可追溯性 |
| 学习产物 | 经验、偏好、提示和技能 | agent 程序乃至学习算法 | 是否需要独立行为评测与更强变更边界 |
| 怎么共享 | 同机、同项目的共用入口 | 云服务、组织仓库、多主体共写 | 一致性、作用域、身份、撤回传播 |

**仍待讨论而非由本报告代定**：用户所说 RSI 到哪一层；跨 agent 共享覆盖不同工具、主从 agent、不同项目，还是不同机器／人。公开资料可以说明这些路径已有何种实现，不能替用户决定 cairn 的范围。

[claude]: https://code.claude.com/docs/en/memory
[claude-sub]: https://code.claude.com/docs/en/sub-agents
[codex]: https://learn.chatgpt.com/docs/customization/memories
[gemini]: https://geminicli.com/docs/tools/memory/
[gemini-guide]: https://geminicli.com/docs/cli/tutorials/memory-management/
[engram]: https://github.com/Gentleman-Programming/engram
[engram-model]: https://github.com/Gentleman-Programming/engram/blob/main/docs/codebase/mental-model.md
[engram-docs]: https://github.com/Gentleman-Programming/engram/blob/main/DOCS.md
[engram-release]: https://github.com/Gentleman-Programming/engram/releases/tag/v3.3.2
[beads]: https://github.com/gastownhall/beads
[beads-release]: https://github.com/gastownhall/beads/releases/tag/v1.3.1
[cmem]: https://github.com/thedotmack/claude-mem
[cmem-arch]: https://docs.claude-mem.ai/architecture/overview
[cmem-config]: https://docs.claude-mem.ai/configuration
[cmem-release]: https://github.com/thedotmack/claude-mem/releases/tag/v13.35.0
[mem0-add]: https://docs.mem0.ai/core-concepts/memory-operations/add
[mem0-migration]: https://docs.mem0.ai/migration/oss-v2-to-v3
[mem0-dream]: https://docs.mem0.ai/platform/features/dream
[mem0-search]: https://docs.mem0.ai/core-concepts/memory-operations/search
[mem0-update]: https://docs.mem0.ai/api-reference/memory/update-memory
[mem0-delete]: https://docs.mem0.ai/api-reference/memory/delete-memory
[mem0-mcp]: https://docs.mem0.ai/platform/mem0-mcp
[mem0-release]: https://github.com/mem0ai/mem0/releases/tag/ts-v3.3.1
[openmemory]: https://mem0.ai/blog/introducing-openmemory-mcp
[graphiti]: https://github.com/getzep/graphiti
[graphiti-mcp]: https://github.com/getzep/graphiti/blob/main/mcp_server/README.md
[graphiti-release]: https://github.com/getzep/graphiti/releases/tag/v0.30.2
[letta-code]: https://github.com/letta-ai/letta-code
[memfs]: https://docs.letta.com/concepts/memfs
[letta-memory]: https://docs.letta.com/configuration/memory
[letta-shared]: https://docs.letta.com/concepts/shared-memory
[letta-release]: https://github.com/letta-ai/letta-code/releases/tag/v0.34.10
[langgraph]: https://docs.langchain.com/oss/python/concepts/memory
[dreams]: https://platform.claude.com/docs/en/managed-agents/dreams
[survey]: https://arxiv.org/abs/2512.13564
[reflexion]: https://arxiv.org/abs/2303.11366
[ace]: https://arxiv.org/abs/2510.04618
[amem]: https://arxiv.org/abs/2502.12110
[lme2]: https://arxiv.org/abs/2605.12493
[arex]: https://arxiv.org/abs/2607.21461
[rsi]: https://sakana.ai/rsi-lab/
[dgm]: https://sakana.ai/dgm/
[poison]: https://arxiv.org/abs/2606.04329
[harness]: https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents
[context]: https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
