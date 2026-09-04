use super::*;

/// 动画路径的高度容差。
///
/// 正常测量用 `measured_height_tolerance_px`（代码块约 1px）过滤 sub-pixel 抖动，
/// 这对静态内容是对的。但补间的尾帧本身就只动零点几个像素——用同一把尺子会把尾段
/// 整批丢掉：下方文档的高度提前冻住，而块内部还在按补间继续缩，一个补间于是被看成
/// 两段。动画期间改用一个只为拦住 f64 噪声的 epsilon。
const ANIMATED_HEIGHT_EPSILON_PX: f64 = 0.01;

/// 这一次该用哪把尺子。
///
/// 由**调用方走的是哪条通道**决定，不去查 `animating_heights`。那个集合的职责只有
/// 一个：拦住动画期间的静态测量。把容差也挂上去的话，一旦 `begin` 静默失败（session
/// 正忙），补间就会退回 1px 容差、尾段又被丢掉——而这种回归是无声的。
fn effective_height_tolerance(animated: bool, kind: &cditor_core::rich_text::RichBlockKind) -> f64 {
    if animated {
        ANIMATED_HEIGHT_EPSILON_PX
    } else {
        cditor_core::layout::measured_height_tolerance_px(kind)
    }
}

impl DocumentRuntime {
    /// 把某个块的高度所有权移交给动画路径，直到 `end_block_height_animation`。
    ///
    /// 期间：`apply_animated_block_height` 可以逐帧落亚像素增量，而 `queue_measured_height`
    /// 对该块一律拒绝——文本布局仍在按自然全高测量并上报，放它进来就会跟补间互抢。
    pub fn begin_block_height_animation(&mut self, block_id: BlockId) {
        self.layout.animating_heights.insert(block_id);
    }

    /// 交还高度所有权。调用方应当在此之前先落权威终值。
    pub fn end_block_height_animation(&mut self, block_id: BlockId) {
        self.layout.animating_heights.remove(&block_id);
    }

    pub fn is_block_height_animating(&self, block_id: BlockId) -> bool {
        self.layout.animating_heights.contains(&block_id)
    }

    pub fn has_dirty_layout(&self) -> bool {
        self.layout.dirty
    }

    pub fn mark_layout_saved(&mut self) {
        self.layout.dirty = false;
    }

    pub fn queue_measured_height(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        height: f64,
    ) -> Result<bool, String> {
        // 该块正在做高度动画：所有权归动画路径。放行的话，文本布局上报的自然全高会
        // 挤进同一张 pending 表（HashMap，同块后写覆盖先写），跟补间高度互抢，折叠
        // 走到一半被拽回全高再拉下去。
        if self.layout.animating_heights.contains(&block_id) {
            trace_image_resize(
                "height.reject",
                format_args!(
                    "block={block_id} version={content_version} height={height:.2} animating"
                ),
            );
            return Ok(false);
        }
        self.queue_height_inner(block_id, content_version, height, false)
    }

    /// `queue_measured_height` 与动画通道共用的校验+入队。
    ///
    /// `animated` 由调用方给定——它就是"这条值来自哪条通道"，也因此决定容差。
    fn queue_height_inner(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        height: f64,
        animated: bool,
    ) -> Result<bool, String> {
        if !height.is_finite() || height < 0.0 {
            trace_image_resize(
                "height.reject",
                format_args!("block={block_id} version={content_version} height={height} invalid"),
            );
            return Err(format!(
                "invalid measured height for block {block_id}: {height}"
            ));
        }
        let Some(payload) = self.document.payload_window.get(block_id) else {
            trace_image_resize(
                "height.reject",
                format_args!(
                    "block={block_id} version={content_version} height={height:.2} payload_missing"
                ),
            );
            return Ok(false);
        };
        let tolerance = effective_height_tolerance(animated, &payload.kind);
        if payload.content_version != content_version {
            trace_image_resize(
                "height.reject",
                format_args!(
                    "block={block_id} requested_version={content_version} current_version={} height={height:.2} stale_version",
                    payload.content_version,
                ),
            );
            return Ok(false);
        }
        let Some(document_index) = self.document.index.index_of(block_id) else {
            return Ok(false);
        };

        let indexed_height = self
            .document
            .visible_index
            .visible_index_of(block_id)
            .and_then(|visible_index| self.layout.height_index.heights.get(visible_index).copied());
        let metadata_height = self.document.index.layout_meta[document_index].effective_height();
        let index_matches =
            indexed_height.is_none_or(|previous| (previous - height).abs() < tolerance);
        let metadata_matches = (metadata_height - height).abs() < tolerance;
        if index_matches && metadata_matches {
            self.layout.pending_measured_heights.remove(&block_id);
            trace_image_resize(
                "height.unchanged",
                format_args!(
                    "block={block_id} version={content_version} indexed={indexed_height:?} metadata={metadata_height:.2} next={height:.2}"
                ),
            );
            return Ok(false);
        }

        self.layout.pending_measured_heights.insert(
            block_id,
            PendingMeasuredHeight {
                content_version,
                height,
                animated,
            },
        );
        trace_image_resize(
            "height.queued",
            format_args!(
                "block={block_id} version={content_version} indexed={indexed_height:?} metadata={metadata_height:.2} next={height:.2} pending={} ",
                self.layout.pending_measured_heights.len(),
            ),
        );
        Ok(true)
    }

    pub fn flush_pending_height_corrections(&mut self) -> Result<bool, String> {
        self.flush_pending_height_corrections_with_priority(HeightCorrectionPriority::Normal)
    }

    pub fn flush_pending_height_corrections_with_priority(
        &mut self,
        priority: HeightCorrectionPriority,
    ) -> Result<bool, String> {
        if self.layout.pending_measured_heights.is_empty() {
            return Ok(false);
        }

        let restore_scroll_anchor = matches!(priority, HeightCorrectionPriority::Normal);
        let viewport_anchor = restore_scroll_anchor
            .then(|| self.target_for_global_offset(self.layout.scroll.global_scroll_top))
            .flatten();
        let pending = std::mem::take(&mut self.layout.pending_measured_heights);
        trace_image_resize(
            "height.flush_begin",
            format_args!(
                "priority={priority:?} pending={} scroll_top={:.2} total={:.2}",
                pending.len(),
                self.layout.scroll.global_scroll_top,
                self.layout.page_layout.total_height(),
            ),
        );
        let mut affected_pages = HashSet::new();
        let mut should_restore_anchor = false;
        let mut applied = false;
        let mut global_height_changed = false;

        for (block_id, pending_height) in pending {
            let Some(payload) = self.document.payload_window.get(block_id) else {
                continue;
            };
            if payload.content_version != pending_height.content_version {
                continue;
            }
            // flush 阶段要认入队时那把尺子，否则尾帧在这里被二次拦下。
            let tolerance = effective_height_tolerance(pending_height.animated, &payload.kind);
            let Some(document_index) = self.document.index.index_of(block_id) else {
                continue;
            };
            let Some(visible_index) = self.document.visible_index.visible_index_of(block_id) else {
                self.document.index.layout_meta[document_index]
                    .update_height(pending_height.height);
                self.layout.dirty = true;
                applied = true;
                continue;
            };

            let indexed_height = self
                .layout
                .height_index
                .heights
                .get(visible_index)
                .copied()
                .unwrap_or_else(|| {
                    self.document.index.layout_meta[document_index].effective_height()
                });
            let metadata_height =
                self.document.index.layout_meta[document_index].effective_height();
            let index_matches = (indexed_height - pending_height.height).abs() < tolerance;
            let metadata_matches = (metadata_height - pending_height.height).abs() < tolerance;
            if index_matches && metadata_matches {
                continue;
            }

            if !metadata_matches {
                self.document.index.layout_meta[document_index]
                    .update_height(pending_height.height);
            }
            self.layout.dirty = true;
            trace_image_resize(
                "height.applied",
                format_args!(
                    "block={block_id} visible_index={visible_index} version={} indexed={indexed_height:.2} metadata={metadata_height:.2} next={:.2} update_index={} update_metadata={}",
                    pending_height.content_version,
                    pending_height.height,
                    !index_matches,
                    !metadata_matches,
                ),
            );
            if !index_matches {
                self.layout
                    .height_index
                    .update_height(visible_index, pending_height.height)
                    .map_err(|error| error.to_string())?;
                global_height_changed = true;
                if let Some(page_index) =
                    self.layout.page_layout.page_for_block_index(visible_index)
                {
                    affected_pages.insert(page_index);
                }
                // 动画块是用户**直接操作**的那个块（点折叠/展开按钮）。它要守的不变量
                // 是"这个块的顶边在视口里别动"——按钮长在块顶，用户的鼠标就停在那儿。
                //
                // 视口顶锚点守的是另一件事："视口顶那一点别动"。当动画块位于视口顶
                // 或其上方时，这两个目标直接冲突：块长高 127px，为了让视口顶那个块
                // 留在原处，scroll_top 必须也 +127，于是块顶（连同按钮）往上跑 127px
                // 冲出视口。实测就是这么跑的。
                //
                // 而块自身变高并不移动它自己的 block_top，所以"块顶不动"恰好等于
                // "scroll_top 不动"：不补偿即正确，下方内容自然被推下去。
                let is_animated_block = self.layout.animating_heights.contains(&block_id);
                if let Some(anchor) = viewport_anchor
                    && visible_index <= anchor.block_index
                    && !is_animated_block
                {
                    should_restore_anchor = true;
                }
            }
            applied = true;
        }

        if !applied {
            return Ok(false);
        }

        if !global_height_changed {
            trace_image_resize(
                "height.flush_end",
                format_args!(
                    "priority={priority:?} metadata_only=true total={:.2} displayed_total={:.2} scroll_top={:.2}",
                    self.layout.page_layout.total_height(),
                    self.layout.scroll.displayed_total_height,
                    self.layout.scroll.global_scroll_top,
                ),
            );
            return Ok(true);
        }

        for page_index in affected_pages {
            let before = self.layout.page_layout.pages[page_index].height;
            self.synchronize_page_after_global_update(page_index)?;
            trace_image_resize(
                "page.synchronized",
                format_args!(
                    "page={page_index} before={before:.2} after={:.2}",
                    self.layout.page_layout.pages[page_index].height,
                ),
            );
        }

        let previous_model_total_height = self.layout.scroll.model_total_height;
        let total_height = self.scroll_extent_height(self.layout.page_layout.total_height());
        self.layout
            .scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        let scrollbar_drag_active = self.layout.scrollbar_drag.is_some();
        if let Some(scrollbar_drag) = &mut self.layout.scrollbar_drag {
            scrollbar_drag.push_pending_height_correction(PendingHeightCorrection {
                old_total_height: previous_model_total_height,
                new_total_height: total_height,
            });
        } else {
            self.layout
                .scroll
                .set_displayed_total_height(total_height)
                .map_err(|error| error.to_string())?;
        }

        if restore_scroll_anchor
            && !scrollbar_drag_active
            && should_restore_anchor
            && let Some(anchor) = viewport_anchor
            && let Some(new_anchor_top) =
                self.layout.height_index.offset_of_block(anchor.block_index)
        {
            // 块内偏移只在该块自身的高度范围内有意义。锚点块本身的高度变了之后
            // （折叠最明显：500 -> 46），旧偏移可能已经指到块尾之外，直接相加会把
            // 视口顶钉进后面的块里。按新高度重新夹一次：夹到块尾等于"视口顶停在
            // 被折叠块的下边缘"，也就是紧跟其后的内容顶到视口顶——正是折叠掉一段
            // 内容后该看到的东西。
            let anchor_block_height = self
                .layout
                .height_index
                .heights
                .get(anchor.block_index)
                .copied()
                .unwrap_or(anchor.offset_in_block);
            let offset_in_block = anchor.offset_in_block.min(anchor_block_height);
            let restored = new_anchor_top + offset_in_block;
            self.layout
                .scroll
                .scroll_to_global_offset(restored, ScrollOrigin::ProgrammaticVirtualScroll)
                .map_err(|error| error.to_string())?;
        }

        trace_image_resize(
            "height.flush_end",
            format_args!(
                "priority={priority:?} total={total_height:.2} displayed_total={:.2} scroll_top={:.2} restore_anchor={should_restore_anchor}",
                self.layout.scroll.displayed_total_height, self.layout.scroll.global_scroll_top,
            ),
        );
        trace_flash(
            "height.flush",
            format_args!(
                "global_change=true total={total_height:.2} scroll_top={:.2} restore_anchor={should_restore_anchor}",
                self.layout.scroll.global_scroll_top,
            ),
        );

        Ok(true)
    }

    pub fn apply_measured_height(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        height: f64,
    ) -> Result<bool, String> {
        if self.queue_measured_height(block_id, content_version, height)? {
            self.flush_pending_height_corrections()
        } else {
            Ok(false)
        }
    }

    /// 补间的每帧高度落到布局。要求该块已 `begin_block_height_animation`。
    ///
    /// 与 `apply_measured_height` 的区别只有容差：这条路走 epsilon，所以尾帧那零点
    /// 几个像素也能落下去，下方内容跟着补间一起滑到底，而不是提前冻住。
    pub fn apply_animated_block_height(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        height: f64,
    ) -> Result<bool, String> {
        if self.queue_height_inner(block_id, content_version, height, true)? {
            self.flush_pending_height_corrections()
        } else {
            Ok(false)
        }
    }
}
