# 任务：cairn 接入 canter

2026-11-11，canter 主控转来需求，用户同意交给 cairn 主控（「你来转吧」）。**草稿，没派。** 待定的几件用户 11-11 都定了，`docs/DESIGN.md` 已按此改（2026-11-11 canter），下一步拆实现任务。

原需求单：canter 主控写的《cairn需求-接入canter》（用户 11-11 看过）。canter 的公开接口以 canter 仓库 `docs/DESIGN.md` 的「钩子（P2-153）」「会话记录的公开读取约定（P2-281）」两节为准，主控读的是 `ed7dc5d`。

## 用户原话

经 canter 主控转述：

1. 「关于接入cairn的事情，我还想问一下，其实就是canter接入cairn之后就是和现在接入cairn的codex和claude code对齐了呗」
2. 「cairn是一个独立的记忆服务。不能因为机器上没有canter就失败」
3. 「等一下，这个不会反依赖cairn吧？」（canter 不为 cairn 写特例，cairn 照 canter 的公开接口接）
4. 「你来转吧」（同意把需求交给 cairn）

## 要的结果

照抄原需求单：

1. canter 成为 cairn 的第三家 agent，和 Claude Code、Codex 一样：
   - 在已采用的项目里开 canter 会话，开局注入摘要和项目的账；
   - 用户发话时照旧补注入；
   - 会话之后由旁路模型写摘要、记账；
   - `cairn status`、`cairn list` 里能看到 canter 的来源。
2. `cairn install|uninstall --agent canter` 装卸 canter 的钩子，和另外两家一样先 `--dry-run` 看、确认才写。
3. canter 是可选的：
   - 没装 canter、没有它的配置目录、canter 服务端没在跑、canter 版本太旧，都不影响 cairn 对 Claude Code、Codex 的工作，不报错；
   - `cairn status` 里 canter 照实显示没装，其余字段不变。
4. Claude Code、Codex 的行为照旧。

不要做（照抄）：不改 canter 仓库，需要 canter 改的写成需求交用户；不读 canter 状态目录里 `config.json` 和 hook 给的 `transcript_path` 以外的文件；测试不碰用户真实的 `~/.canter`，用隔离的 `CANTER_HOME`。

以后（不在这次）：paddock 的 Cairn 面板接 canter，由 canter 主控另写需求给 paddock；所以 `status` 里 canter 那项按兼容新增写进 §7.1 公开约定。

## 怎么算做完

以上面“要的结果”为准，另加下面三条（主控提议，用户 11-11：「三条验收都加上」）：

1. 没装 canter 的机器上，现有测试全过，`status --json` 里 claude、codex 两项和改前一样。
2. 在 canter 子会话里打字，不产生新来源。
3. 回退掉已写进摘要的轮之后，下一版摘要里没有那几轮的内容。

## 现状（主控核对过）

canter 现在（11-11 起）已经满足 P1-17 列的三件：开局能注入（`hookSpecificOutput.additionalContext`）；有 SessionStart、UserPromptSubmit、Stop、SessionEnd 四个事件；每个事件都给 `session_id`、`cwd`、`transcript_path`。账里 #45“cairn 目前不能接入 canter”已过时。

cairn 这边要动的：

- `cairn hook <agent>`、`install`、`status`、来源前缀（`canter:<session_id>`）、会话记录解析器，现在都只认 `claude`、`codex`。
- **要升库**：`hook_seen` 表有 `CHECK (agent IN ('claude','codex'))`，记不进 canter 的最近触发时间，得升到版本 6。装之前照旧 `sqlite3 .backup` 备份；升了之后退回旧版要连库换回。
- canter 的配置只有全局 `$CANTER_HOME/config.json`（没设就是 `~/.canter/config.json`），JSON 格式，`hooks` 写法和 Claude Code 一样，现有的 JSON 合并逻辑能复用。结构写错会让 canter 整份配置作废，所以只合并 cairn 自己的条目，原样保留其余内容。
- canter 没有 Codex 那种 hook 信任审核；目录没被信任（受限打开）时 canter 不跑任何钩子。改了配置下一轮生效，不用重启。

## 要 cairn 拿主意的三件（canter 主控提的）

### 1. 没有回合 ID：不需要，canter 不用改

核对了代码：回合 ID（Claude 的 `prompt_id`、Codex 的 `turn_id`）只在 UserPromptSubmit 时当备注存进 `events.detail`，本来就允许为空；按它判定的 `turn_decisions` 从 10-13 起不再读写。写摘要、记账、补注入都不看它。canter 这一项留空即可。

### 2. 子会话（带 `agent_id`）的事件：建议不算

canter 子会话只发 UserPromptSubmit（用户直接在子会话里打字时），不发 SessionStart、Stop、SessionEnd。要是照普通会话处理，cairn 会给子会话单独记一个来源，之后在同一条工作线有新会话开局时给它写一份单独的摘要，和主会话的摘要并列注入，内容是子任务的片段。

主控建议：带 `agent_id` 的事件一律不处理，不记、不注入。主会话的会话记录里有派活和收结果的经过，摘要从主会话来。Claude Code 的子 agent 现在 cairn 也不单独记。

### 3. 工具输出：建议只读公开约定里的行

现在 Claude、Codex 的解析器会抽“调了哪个工具、参数、结果”（各截到 500 字）。canter 公开约定里只有用户消息、模型回复；工具行不在约定里，以后可能变。

主控建议：canter 的解析器只读公开约定里的行（用户消息、已收尾的模型回复，加上回退、撤回）。代价是 canter 会话的摘要和账看不到具体跑了什么命令、测试输出，只能靠模型回复里的叙述。试用一段时间觉得不够，再写需求请 canter 把工具调用加进公开约定，而不是现在去读不在约定里的行。

## 主控另外发现的两件

### 4. 断线重连的那次 SessionStart 不注入，cairn 分不出来

canter 打开存盘会话、客户端断线超过 10 秒后重连，两种都发 `SessionStart`、`source: "resume"`；前一种注入，后一种只通知、丢掉输出。stdin 里没有字段能区分。

cairn 对 resume 的做法（§8.1）是：注入这个来源还没见过的摘要和现场对比，并记下“已注入”和账的版本号。碰到重连那次，cairn 以为注入了，其实被丢了，之后用户发话时的补注入也不会再补这些摘要和账的变动。canter 是客户端/服务端分开的，客户端关掉过一会儿再连回来应该挺常见，所以这个漏洞不小。

办法，三选一（主控推荐 A）：

- **A**：写一份需求交 canter，让重连那次的 SessionStart 带上能区分的字段（比如 `source` 用新值，或者加一个表示“这次输出不会注入”的字段，怎么加由 canter 定）。canter 加之前，cairn 先照现在的 resume 做法接，这个漏洞写进 DESIGN §14。需求草稿已写好，交用户转给 canter（不放进本仓库）。
- **B**：cairn 对 canter 的 resume 一律不注入、不记“已注入”，全交给用户发话时的补注入。不用等 canter，但打开存盘会话时就看不到现场对比了（canter 会重放上次开局时的注入，摘要不会全丢）。
- **C**：等 canter 改了再接 canter。

### 5. 回退、撤回：写摘要和记账要按“当前有效的对话”来读

canter 回退、撤回是在文件后面追加一行，原来的消息行还在。cairn 现在是从上次读到的字节位置往后读（§8.8），给旁路模型“上一版摘要加新增部分”。如果新增部分里有一行回退，撤掉的可能是上次已经写进摘要、记进账的那几轮。

主控的做法（cairn 自己的事，不用 canter 改；用户有异议再说）：

- canter 的解析器每次从头读整个文件，按公开约定去掉被回退、撤回的轮，得出当前有效的对话，再按上次读到的位置切出新增部分。
- 写摘要：发现上次已写进摘要的轮被去掉了，就不给上一版，用当前有效的全文重写。
- 记账：只给新增部分里有效的轮。被撤掉的那几轮之前已经记进账的，不自动撤，写进 DESIGN §14；用户发现了用 `cairn ledger` 改。

## 其余照原样（主控的理解）

- 来源是 `canter:<session_id>`；`cairn status --json` 加 `agents.canter`，字段和另外两家一样（`installed`、`last_seen` 四个键），按 §7.1 的“只加不改”写进公开约定。没有 canter 配置目录或没装时 `installed` 是 `false`、`last_seen` 全是 `null`，不报错。
- `cairn install --agent canter`：没有 canter 配置目录时拒绝，给一行原因，不替 canter 建目录。不检查 canter 版本；版本太旧（不认 `hooks` 或不认 `additionalContext`）的，`status` 里 `last_seen` 一直是空或只有通知事件，照实报。
- canter 的 StopFailure（轮出错）、Interrupt（被打断）不处理，和 Claude、Codex 一样：会话安静 3 分钟、会话结束或同一工作线有新会话开局时照样写摘要。
- 命令写绝对路径（稳定软链接），canter 钩子的 PATH 来自客户端，这点照旧满足。corral 带 `CAIRN_DISABLE=1` 开的 canter，钩子里也看得到这个变量，照旧不处理。
- 写摘要、记账照旧只用 Codex（D6）。
- `cairn save` 在 canter 的沙箱里能不能写进暂存区，没核对；实测时看。save 是自愿的，写不了不影响摘要。

## 要改的设计（11-11 已改）

已改（标“2026-11-11 canter”）：§1、D10、新增 D16、§3 回合、§4、§5、§6.2 版本 6、§7、§7.1 `agents.canter`、§8.8 canter 解析器和回退的处理、§10.3、新增 §10.5 canter、§12、§13、§14 R28–R30、§15。

## 实测

`docs/背景与决策记录.md` 写着“其他 agent 每接一个，都要单独实测”。canter 要实测的：开局注入、发话时补注入真的进了模型、子会话不被记、回退之后的摘要、`cairn save` 能不能写进暂存区。

难处：canter 只有全局配置，没有会话级注入钩子的办法；隔离的 `CANTER_HOME` 里没有登录，canter 调不了模型。怎么测见“用户 11-11 定的”第 4 条。

## 用户 11-11 定的

1. 子会话：不算。
2. 工具输出：只读公开约定里的行。
3. 断线重连：A。需求草稿交用户转给 canter；cairn 先照现在的 resume 做法接，漏洞写进 §14。
4. 实测：隔离的 `CANTER_HOME`，用户在里面登录一次，测完删目录，不碰真实 `~/.canter`。预计开 1 个 canter 会话，交互测几轮。
5. 验收：主控提议的三条都加上（用户：「三条验收都加上，接着改设计」），见“怎么算做完”。
