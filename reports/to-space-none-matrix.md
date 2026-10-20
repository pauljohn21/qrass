# to-space none 传播 — Wave 0 证据矩阵与止损结论（plan: to_space_none_propagation）

## 提取统计（2844 sections / 257 hrx，326 FAIL @ hrx-round87c）
FAIL 按 section 名分布：alpha/partial 88、out_of_range/far 44、out_of_range/near 35、
missing/{blue21,green15,red8,a7,lightness6,hue4,non_hue6,non_lightness4,all4}、gray 13、middle 10、float 8。

## 核心判决（全部由 golden 证据对归纳）
1. **主簇 = 1e-8 相对尾差**，非 none 依赖模型：
   - 同输入 `color(xyz 0.2 0.4 0.8)`→lab：ours `68.6381333077`，golden `68.6381340629`（Δrel 1.1e-8）
   - a98→lab 与 alpha/missing 变体差分同量级同符号 —— **一个常数尾差源，全链共担**
   - pre-multiply（结合律）对照实验：seq/comb 输出 bit 级一致 → 差异常数舍入本身，非操作序
2. **none 语义已正确**：gamut↔gamut 位置传播（r↔x）与 missing→0 计算均与 golden 一致
   （a98→lab missing/green|blue|all 全 PASS）。
3. **out_of_range/far 双重差异**：(a) 目标非三刺激空间时 golden 输出
   `color-mix(in {tgt}, color(xyz …) 100%, black)` 形态；(b) far 端指数位数量级差
   （ours 9.0e12 vs golden 9.1e13）——极值域 gamma 曲线不同，需独立考古。
4. in-range PASS 与 FAIL 的分界 = 12 位有效数字显示的舍入幸运位（非模型分界）。

## 止损（plan §风险条款触发）
「证据表无法自洽」条件成立：尾差需反推 dart 常数（无本地证据抓手，禁读其内部机制），
far 需曲线+形态+指数三层考古。**本专项不产出代码改动**；318 例记债，
若未来 sass-spec 提供更高容差模式或新 golden 集再启。

## 可安全实现子集（已核查为 0）
- alpha/missing/near 全族：依赖尾差，不可单独修
- far 族：依赖 (b) 指数差异，仅 (a) 形态不够
- 无 none 模型类快赢可收
