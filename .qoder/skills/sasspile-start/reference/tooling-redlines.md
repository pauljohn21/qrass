# 工具级红线：验收读数也会骗人、A/B 记录只住 ledger、量具覆盖面与阳性对照 — 从 `.qoder/rules/rust.md` 整段迁入，此处为唯一副本

> 由 `.qoder/rules/rust.md` 迁出（2026-10-28）。

### 🧪 验收读数也会骗人（工具级红线）
- `crates/ledger` 只存失败行；同名 run 重导已改为先 `DELETE FROM run_fail`（旧版会把“已修好”报成 phantom REGRESS）。
- 断言 `regress=0` 前 MUST 在**另一个空库**重做一次 A/B（`HRX_DB=<临时路径> HRX_RUN=<名> cargo test -p sasspile --test hrx`）；
  行为不变型重构改为两侧各入同一临时库后 `ledger diff` 证 **net=0**（快照文件已不存在，md5/`diff` 无对象可用）。
- “同名两结果完全相同”得到的 `regress=0` 不证明任何事； 行为变更 MUST 列出**哪几条用例**变了及其规范依据。
  “FAIL 变成另一种 FAIL”在 pass 读数上看不见 ⇒ MUST 另数无效错误：`ledger failures <run> "unexpected error"`
  （它同时匹配 key 与失败详情；这个查询曾因 `LIKE` 绑定 + 吞错而永远返回 0，现由
  `crates/ledger/tests/failures_query.rs` 钉住。长期基线曾是 **53**，2026-10-28 把 `module.$name` 做成词素后
  降到 **20**（那 33 行本来就是把内置模块常量当未绑定变量报错，不是新能力失效）——引用它之前先跑一次读数，别拿旧数）。
- **通过率必须同时报“实质通过”**：`tests/hrx.rs` 会输出 `pass_substantive`（期望 CSS 非空而通过）与
  `pass_empty_expectation`（期望输出为空，“啥也不输出”也能对）。当前 746 PASS = **581 实质 + 165 空期望**。
  实证：`directives/{if,for,while}` 簇的 39 条“PASS”**全部**是空期望 ⇒ `@if`/`@each`/`@for` 其实一行也没实现，
  而 `tasks.md` 3.2 却写着“已做”（从已删的 v1 流水线带过来的假状态，已更正）。**声称某特征能用时，
  MUST 看该簇的非空期望通过数，而不是看总 pass。**
- **用例 key MUST 唯一**：旧 `tests/hrx.rs` 用 `<basename>.hrx::<case>` 做 key，而语料在不同目录重名（两个
  `function.hrx`），11,486 条里实测 **1,135 个重复 key / 33 个同时挂着 PASS 与 FAIL** → 一个文件的通过会
  掩盖另一个的失败（一度把回归归到错误的文件上）。现在 key = 相对 `spec/` 的全路径，且 harness **有撞名断言**；
  旧制式的 run 与新 run **不得跨制式 diff**（`reports/hrx.db` 的 `case-key-scheme` note 已记明）。
- **“报了个错”≠“按金样报错”**：旧 harness 把任意 `Err` 当 `error` 用例通过，使 `FATAL` 翻转虚报 **+385**；
  逐条比对金样 `Error:` 首行后真值是 **+1**（384 条的期望消息是 `Expected identifier.`、单位错误、
  `DEPRECATION WARNING`——它们属 P1/P7 待实现的消息，不是错误管道）。批量新增/回退 MUST 抽样看**期望文本**，
  并区分“指标修正”与“真收益”。
- **A/B 记录只住 `reports/hrx.db` 的具名 run，且不再经任何中间文件**：需要入库时用
  `HRX_RUN=<增量名> cargo test -p sasspile --test hrx`，harness 直接写库（`HRX_DB=<路径>` 指向另一个库，
  供强制的空库复算用）；不设 `HRX_RUN` 时只量不写、不留任何产物。事后靠 `ledger runs` / `ledger diff <a> <b> [簇前缀]` /
  `ledger failures <run> [key-或-详情子串]` / `ledger passed <run> <簇前缀>` 追溯。
  **读数一律走 `cargo run -q -p ledger -- …`：既禁止 `sqlite3` 裸查记录库，也禁止在 shell 里用
  `sed|sort|comm|awk` 对 ledger 的输出重算集合差**（本轮两次违规：先手拼 SQL 反推 PASS 集，后又在
  `/tmp` 导两个 key 列表用 `sort|comm` 算簇级 A/B —— 后者正是本 crate 要消灭的 `reports/hrx-*.tsv`
  sprawl 换地方复发。**缺哪个读数就补哪个读数，而不是绕它**）。本轮据此补了两个命令，并把结论做成
  `pub fn failing/passing/diff_runs` 让测试直接断言（不再 grep 日志）：
  • `passed <run> <簇前缀>` —— 库**只存失败行**且**不记某 run 的测量范围**（in-scope 的 PASS 只登记 key、
    不长行），“这簇现在哪些是 PASS”无处可答；用“全部已知 key − 该 run 失败”去反推会把**从没测过的用例
    报成通过**（实测 `passed head-600-pathkey operators/` 返回 29 条，而那一版 harness 压根没扫 operators），
    故**簇前缀必填**、覆盖面由调用方按语料自证（`tests/passing_query.rs` 钉住）；
  • `diff <a> <b> [簇前缀]` —— 验收口径本来就是**按簇 scoped**（每簇自基线只要求不回退），无簇级读数就
    只能手拆；`Diff{fails_a,fails_b}` 是“簇内根本没用例可比”与“簇内 0 回归”的区分器
    （`tests/scoped_diff.rs` 钉住）。
  另：**`sasslint lint <dir>` 曾把 `lint` 当路径扫**（取 `argv[1]`），故文档化的 CLI 闸**永远报
  `count=0`**，同期 `gate_sasspile_is_clean` 却是红的 —— 永远通过的闸比没闸更坏，已修（吞动词、路径不存在
  即 exit 2）并由 `crates/sasslint/tests/cli_directory.rs` 钉住。
  MUST NOT 再写回“先导出 TSV 再 `ledger import`”的做法，也不得开第二个库当记录。
  **MUST NOT** 攒快照文件或第二个 sqlite（`crates/ledger` 的存在理由就是取代旧的 `reports/hrx-*.tsv` sprawl；
  本轮曾误把基线存成 `target/baselines/*.tsv`，15 个文件 / 27 MB，已全部入库并删除）。
  为验收新建的空库（`--db=<临时路径>`）只用于复算，**不作为记录留存**；`/tmp` 会被系统清理。
- **任何“为 0 / 全通过”的统计读数 MUST 先过一条阳性对照**（已知应该为真的用例）。本轮实错：一个
  `substr` 切片多吞一个引号字符，使“仅差空白/仅差大小写”都报 0，差点得出“无 cheap win”的结论。同类第二次：
  `ledger passed <run> operators/` 对**根本没扫过该簇的** run 报 29 条“通过”（库不记测量范围）。新增读数
  自己也要带阳性对照与“匹配为空”两条测试，否则它只是另一个会静默失效的量具。
- **`HRX_DB` 的相对路径按 crate 目录解析**：`HRX_DB=target/x.db` 会变成`crates/sasspile/target/x.db`，因目录不存在而
  直接报 `unable to open database file`（好在是 exit 1，不是静默错）。空库复算一律用**绝对路径**。
- **harness 不比对 `warning` 段**：不少行同时期望一条 deprecation 警告（如 `core_functions/modules/color/hue::`
  期望 `color.hue() is deprecated`）。这类行“输出对、警告缺”，**不得据此宣称 deprecation 已实现**；
  单例文件（section 直接叫 `input.scss`）的 key 尾部为空（`…hue::`）是合法的，不撞名。
- **链式量具命令先确认 `pwd`**：shell 会话会复用，cwd 会漂到子模块里（本轮实际发生：漂进 `sass-spec/spec`）。
  在这种目录下用相对路径读写会静默写错地方或读不到文件，而**用相对路径去读语料恰好仍然成立**，
  所以错误不会报错只会被隐藏——路径一律用绝对路径或先 `cd` 到仓根。
- **绝不在命令链里放可交互程序**（`python3 -v` 会拉起 REPL 吞掉整个终端，后续命令全被当成 Python 语句
  吃掉）；发生时的恢复法：另开一个新终端跑命令（旧终端里的垃圾输入会随它一起报废），**不要**用补引号
  的方式去“救”——那只会制造新的未闭合引号，本轮连错两次。
- **量具覆盖面 MUST 等于计划引用的簇**：`tests/hrx.rs` 的 `SPEC_DIRS` 长期缺 `operators`/`parser`/`variables`
  （共 79 例未计量），而任务 1.6 自己写着“经 `operators/**` 金样验”——引用了却扫不到，等于用不存在的
  判据宣称 regress=0。目录清单以 `sasspile-conformance` 的 hrx harness 契约为准；扩面算范围变更，验收判据是
  **旧覆盖集内不得有 PASS→FAIL**，不是总数上涨（扩面后基线 600 → 616，总数 11,533）。

