# 任务：记账的模型输出改成 JSON，原话里有竖线、反斜线时不再因格式失败

2026-10-17，cairn/main 交给 cairn/dev-ledger-json（Codex，常规：gpt-6-astra / medium）。
路由：常规 / 交叉审查不要 / 影响面：改行为（路由：常规（置信 0.39）、交叉审查拿不准（核心规则 0.48）、影响面拿不准；主控判：检查规则全保留、不动数据库，按改行为）
类型：Bug 修复
依据：本轮只改记账那一步的模型输出格式和解析；写摘要那一步、数据库结构、截断规则、开局注入、公开约定都不动。
提示：依据证据定位并修复导致问题的原因，保持无关行为不变。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 起因和用户原话

2026-10-17 装上 0.7.0 后，主控用 `cairn ledger runs` 查了几个仓库：ranch 4 次记账全部失败，paddock 7 次失败 2 次，原因是“输出不合格：第 N 行：字段数不对 / 转义不支持 / 空行”。要求里让模型“依据照抄原话”，原话里有 shell 管道、`grep "a\|b"` 这类竖线和反斜线，模型手写 `\|`、`\\` 转义常出错，一行错整批丢。同一份摘录重试两次，报的错不一样。

主控提议改成 JSON，用户：“好的，写任务书派出去。然后走一段时间看看，不行就换模型。”

主控 10-17 用合成材料实测过：本机 PATH 里的 `codex`（包装脚本）能透传 `codex exec --output-schema <文件>`，`gpt-5.6-luna` / medium 照 schema 输出了 JSON，依据里的竖线、反斜线、引号都正确保留；`edit` 那笔它也顺手填了 `reason`。

## 先读

- `AGENTS.md`（规矩、编译目录、隐私）。
- `docs/DESIGN.md` §8.9“模型输出”“检查”和“留痕”里“输出不合格写明哪里”那条、§9.6（这次的设计，主控已在 main 改好，以它为准）。
- `crates/cairn/src/ledger/sidecar.rs`（`INSTRUCTIONS`、`prompt`、`parse`、`fields`、`validate`）、`crates/cairn/src/sidecar.rs`（调 `codex exec` 的那段，约 770 行起；记账怎么调它）。
- 测试写法参考 `crates/cairn/tests/sidecar.rs`、`crates/cairn/src/sidecar/tests.rs`（合成会话记录、假 `codex`、隔离目录）。

## 在哪里干活

- worktree：`../cairn-worktrees/p1-26-ledger-json`，分支 `p1-26-ledger-json`（已从 main 建好）。
- 编译目录：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target/p1-26-ledger-json`。

## 要做的

照 DESIGN §8.9、§9.6：

- 记账那一次 `codex exec` 加 `--output-schema <文件>`；schema 文件每次写进这次调用的临时目录，调完一起删。写摘要那一次不加，不变。
- 输出是 `{"changes":[…]}`，每笔七个键 `op`、`kind`、`no`、`title`、`body`、`reason`、`evidence`。schema 按 Codex 严格模式写（键全列必填，用不到的允许 `null`，`additionalProperties: false`）。
- 解析改成读 JSON：空数组等于原来的 `NO_CHANGES`；**各操作用不到的键不论填了什么都忽略**；`null` 的标题、理由、依据当空，`null` 的正文当空串。程序自己检查形状，不依赖 Codex 守住。
- 原有的检查都保留：类、编号、长度、账 10 笔 / 经验 5 笔、两组分开、同一条既改又了结、128 KiB 上限、版本冲突重来。
- 失败原因从“第 N 行”改成“第 N 笔”（`changes` 里第几个，从 1 起）；整份解析不了或形状不对写“输出不合格：不是合格的 JSON”。不抄输出内容。
- `INSTRUCTIONS` 照 §9.6 改：去掉手写转义和行格式那几句，加 JSON 那几句和“当前的账里 `\n`、`\|`、`\\` 是写法，输出写原字符”那句。“当前的账”的展示格式不变。
- 假 `codex` 和相关测试改成新格式；测试里要有依据带竖线、反斜线、引号、换行的一笔，能原样写进账。

## 怎么算做完

- 记账模型的输出改成 JSON，原话里有竖线、反斜线、引号、换行时，照原样写进账，不再报“字段数不对 / 转义不支持 / 空行”。
- DESIGN §8.9、§9.6 写的行为都做到；实现时觉得设计哪里要改，先停下来报告，不要自己改 DESIGN。
- 验证只做这些：一条针对这件活的测试（先写，确认在旧代码上因为格式问题失败），再跑一次 `cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings`。觉得不够，在回复里说，不要自己加。

## 不要做

- 不动写摘要那一步、数据库结构（不升版本）、截断规则、开局注入、`status` / `show` / `list` 的公开约定字段。
- 不保存模型原始输出。
- 不调用真实 `codex`，测试一律用假 `codex`。不读本机真实的会话记录、cairn 数据库、各工具的记忆文件。不改 `~/.claude`、`~/.codex` 下的配置，不运行 `cairn install`。
- 不改版本号（主控装的时候改）。
- 不要按项目名或路径批量杀进程（`pkill -f cairn` 这类）：主控和别的 agent 的进程命令行里都带着项目名和工作目录。停自己起的进程用启动时记下的 PID。
- 不合并到 main，不推送。只在 `p1-26-ledger-json` 上提交。

## 做完

在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事、踩过的坑，各几句话。踩过的坑指试了不行、换了办法的，工具或环境的行为和预想不一样的；没有就写「没有」，不要凑。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 完成记录

- 做了什么：记账调用加入 `--output-schema`，严格 schema 随每次调用写入临时目录并一起清理；模型提示词同步 DESIGN §9.6。解析改为 JSON 对象与 `changes` 数组，按操作忽略不用的字段，处理 `null` 和首尾空白，保留原有分组、上限、长度、有效编号与版本冲突检查。诊断改成“第 N 笔”，JSON 解析或形状错误统一报“不是合格的 JSON”。假 codex 和原有夹具同步新格式。
- 验证了什么：先新增 `ledger_json_preserves_original_evidence_characters`，在旧实现上运行，因记账解析失败而 RED；改后在全量测试中通过，依据和正文中的竖线、反斜线、引号、换行原样入账，同时验证不用字段可带任意值、schema 必填键及临时 schema 清理。按任务只运行一次 `cargo test --all-targets`：到 sidecar 测试集为止，前面 115 项通过，sidecar 31 项通过、2 项失败，后面的测试集因 cargo 停止而未运行。失败一是空输出夹具仍期待旧“没有内容”（现已改成“不是合格的 JSON”，未重跑）；失败二是未改动的 `sidecar_summary_still_reports_session_end_observed_before_generation` 未观测到 SessionEnd，原因未进一步定位。修正夹具后的 `cargo clippy --all-targets -- -D warnings` 通过。没有扩大验证或调用真实 codex。
- 拿主意的地方：JSON 形状检查要求顶层只有 `changes`、每笔七键齐全，无额外键；各操作不用的键只要求存在，不检查值或类型。schema 判断复用调用已有的记账输出上限区分，摘要调用参数不变。没有修改 DESIGN。
- 没做的事：未改摘要行为、数据库、版本、截断、注入和公开字段；未读取真实会话、真实数据库或工具记忆，未改真实配置、安装、合并或推送。未补跑测试；需要主控决定是否安排修正后的全量复验及 SessionEnd 失败排查。
- 踩过的坑：替换旧协议夹具时漏了空输出的旧诊断断言，全量测试暴露后已修正；另有上述 SessionEnd 观测测试失败，未改断言、超时或无关实现来绕过。全量测试未全绿，不能作为合并验证通过的记录。
