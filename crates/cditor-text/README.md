# cditor-text

`cditor-text` 是 Cditor 的框架无关文本布局边界，也是 workspace 中唯一可以直接依赖 Parley 的 crate。

职责：

- shaping、字体 fallback、Bidi、换行和对齐。
- 不可变 `TextSnapshot` 及 UTF-8、UTF-16、grapheme、shaping cluster 映射。
- 构建期物化 `TextGeometrySnapshot`：logical line bounds、visual clusters、每个合法
  UTF-8 标量边界的 upstream/downstream caret stop，以及 selection/hit-test 索引。
- caret、selection rect、普通点击 hit-test 全部查询该不可变几何快照；word/line
  selection 和 visual navigation 继续使用同一 snapshot 内 Parley layout 的语言语义。
- inline style、inline box、AccessKit 文本投影。
- 不可变 layout snapshot、surface-aware cache key、reflow，以及与 geometry 同次发布的
  exact font instance + glyph id/position paint plan。
- 条数/估算字节双预算 LRU、editing/visible/overscan/offscreen 优先级、pin 和内存压力淘汰。
- 显式 relayout strategy：cache hit、仅 reflow，或带 content/style/inline/font/scale 原因的 full build。
- 保留字体 blob、collection face index、variation coordinates、synthesis 和 glyph identity，交给 App paint adapter。
- `FontInstanceKey` 用 fontique blob runtime identity/长度、face index、normalized
  coordinates 和 synthesis settings 唯一标识当前进程内的字体实例；需要跨进程证明时按需计算
  SHA-256，避免在普通 layout/paint 热路径扫描大型系统字体文件。
- 支持显式注册内存字体；注册成功后清空当前线程 layout cache，避免继续使用注册前的 fallback snapshot。
- inline box 的 id/kind/位置/尺寸进入 immutable snapshot，App renderer hook 消费同一份位置数据。

不负责：

- GPUI window、focus、input handler 或绘制调用。
- 文档 selection、IME composition、transaction、undo。
- Block 结构、虚拟滚动、存储或同步。

验证：

- 版本化 corpus：`tests/fixtures/text-layout/v1/`。
- 覆盖 CJK、emoji ZWJ、combining、Arabic、Hebrew、mixed Bidi 和 League Spartan variable `wght` axis。
- manifest 固化 schema、相对路径、字体 SHA-256、axis 范围和 OFL 许可证。
- Proptest 生成 Unicode token、宽度与 scale，验证同一点在 immutable snapshot 上重复执行 point -> index -> caret bounds 的结果稳定且漂移不超过 1 device pixel。
- Parley oracle 逐 UTF-8 边界、affinity、二维采样点和任意 boundary range 对照物化后的
  caret、hit-test 与 selection rect，并覆盖空文本、soft wrap、mixed Bidi、ZWJ 和 inline box。
- `goldens/visual-layout-v1.json` 使用 vendored exact font，在 1x/1.25x/2x 下以
  1/64 device pixel 固化 line break、glyph identity/position、caret affinity、
  selection fragments、underline 和 background。

```bash
cargo test -p cditor-text --lib
cargo clippy -p cditor-text --lib --tests --no-deps -- -D warnings
./scripts/dev/check_structure.sh
```

确认 Parley 或字体升级产生的视觉变化正确后，显式更新 golden：

```bash
CDITOR_UPDATE_TEXT_VISUAL_GOLDEN=1 \
  cargo test -p cditor-text visual_layout_matches_versioned_golden
```

性能基准使用无额外框架依赖的 `bench` profile harness，输出 p50/p95/p99 和机器可读 JSON：

```bash
cargo bench -p cditor-text --bench text_layout -- --quick
cargo bench -p cditor-text --bench text_layout
cargo bench -p cditor-text --bench text_layout -- --full
```

`--full` 使用精确 10 MiB code fixture。基准会对 focused reflow 和 100 个 cached
visible surfaces 执行预算检查；large-code 数据用于约束后续内部切片/虚拟化，当前不能据此宣称
10 MiB 整块同步布局已达到输入帧预算。

调试 Parley 与 GPUI 的字体桥路由：

```bash
CDITOR_TRACE_INPUT=1 cargo run -p cditor-desktop
```

静态 face-0 只有在 exact blob 注册与 glyph ID 顺序/数量均验证成功时才走 GPUI glyph
atlas。TTC face、variable coordinates、synthesis、系统 family 来源不明和 glyph mismatch
会自动走基于 Parley 原始 font instance 的 exact raster image-atlas 路径；报告同时包含
cache 命中与首个失败的 blob/face/glyph 身份。focused text trace 还会输出
range/point/navigation snapshot query、同步最小布局 fallback、unavailable 和累计
geometry fallback rate。
