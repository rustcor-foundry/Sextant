use accesskit::Role;
use masonry::widget::WidgetMut;
use masonry::{
    Action as MasonryAction, AccessCtx, AccessEvent, BoxConstraints, EventCtx, LifeCycle,
    LifeCycleCtx, PaintCtx, PointerEvent, Size, StatusChange, TextEvent, Widget, WidgetPod,
};
use smallvec::SmallVec;
use std::any::Any;
use std::sync::Arc;
use vello::Scene;
use xilem::{MasonryView, MessageResult, ViewCx, ViewId};

use crate::state::SextantState;

#[derive(Clone)]
pub struct PollerTick;

pub struct AsyncPoller {
    pub should_poll: bool,
}

impl Widget for AsyncPoller {
    fn on_pointer_event(&mut self, _ctx: &mut EventCtx, _event: &PointerEvent) {}

    fn on_text_event(&mut self, _ctx: &mut EventCtx, _event: &TextEvent) {}

    fn on_access_event(&mut self, _ctx: &mut EventCtx, _event: &AccessEvent) {}

    fn on_status_change(&mut self, _ctx: &mut LifeCycleCtx, _event: &StatusChange) {}

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle) {
        match event {
            LifeCycle::WidgetAdded if self.should_poll => ctx.request_anim_frame(),
            LifeCycle::AnimFrame(_) if self.should_poll => {
                ctx.submit_action(MasonryAction::Other(Arc::new(PollerTick)));
                ctx.request_anim_frame();
            }
            _ => {}
        }
    }

    fn layout(&mut self, _ctx: &mut masonry::LayoutCtx, _bc: &BoxConstraints) -> Size {
        Size::ZERO
    }

    fn paint(&mut self, _ctx: &mut PaintCtx, _scene: &mut Scene) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx) {}

    fn children(&self) -> SmallVec<[masonry::widget::WidgetRef<'_, dyn Widget>; 16]> {
        SmallVec::new()
    }

    fn make_trace_span(&self) -> tracing::Span {
        tracing::info_span!("AsyncPoller")
    }

    fn get_debug_text(&self) -> Option<String> {
        Some("AsyncPoller".to_string())
    }

}

#[derive(Clone)]
pub struct AsyncPollerView {
    should_poll: bool,
}

impl MasonryView<SextantState> for AsyncPollerView {
    type Element = AsyncPoller;
    type ViewState = ();

    fn build(&self, cx: &mut ViewCx) -> (WidgetPod<Self::Element>, Self::ViewState) {
        cx.with_leaf_action_widget(|_| {
            WidgetPod::new(AsyncPoller {
                should_poll: self.should_poll,
            })
        })
    }

    fn rebuild(
        &self,
        _view_state: &mut Self::ViewState,
        _cx: &mut ViewCx,
        _prev: &Self,
        element: WidgetMut<Self::Element>,
    ) {
        element.widget.should_poll = self.should_poll;
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        id_path: &[ViewId],
        message: Box<dyn Any>,
        app_state: &mut SextantState,
    ) -> MessageResult<()> {
        debug_assert!(id_path.is_empty(), "id path should be empty in AsyncPollerView");
        match message.downcast::<masonry::Action>() {
            Ok(action) => {
                if matches!(*action, masonry::Action::Other(_)) {
                    app_state.drain_async_results();
                    MessageResult::Action(())
                } else {
                    MessageResult::Stale(action)
                }
            }
            Err(message) => MessageResult::Stale(message),
        }
    }
}

pub fn async_poller(should_poll: bool) -> AsyncPollerView {
    AsyncPollerView { should_poll }
}
