use crate::{
    AnyEffect, AnyMemo, AnySignal, EffectId, MemoId, MemoState, Scope, ScopeDisposer, ScopeId,
    ScopeState, SignalId, SignalState, Subscriber, TransitionState,
};
use slotmap::SlotMap;
use std::cell::RefCell;
use std::fmt::Debug;

#[derive(Default, Debug)]
pub(crate) struct Runtime {
    pub(crate) stack: RefCell<Vec<Subscriber>>,
    pub(crate) scopes: RefCell<SlotMap<ScopeId, ScopeState>>,
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scope<T>(&self, id: ScopeId, f: impl FnOnce(&ScopeState) -> T) -> T {
        if let Some(scope) = self.scopes.borrow().get(id) {
            (f)(scope)
        } else {
            panic!("couldn't locate {id:?}");
        }
    }

    pub fn any_effect<T>(
        &self,
        id: (ScopeId, EffectId),
        f: impl FnOnce(&Box<dyn AnyEffect>) -> T,
    ) -> T {
        self.scope(id.0, |scope| {
            if let Some(n) = scope.effects.borrow().get(id.1) {
                (f)(n)
            } else {
                panic!("couldn't locate {id:?}");
            }
        })
    }

    pub fn any_memo<T>(&self, id: (ScopeId, MemoId), f: impl FnOnce(&Box<dyn AnyMemo>) -> T) -> T {
        self.scope(id.0, |scope| {
            if let Some(n) = scope.memos.borrow().get(id.1) {
                (f)(n)
            } else {
                panic!("couldn't locate {id:?}");
            }
        })
    }

    pub fn memo<T, U>(&self, id: (ScopeId, MemoId), f: impl FnOnce(&MemoState<T>) -> U) -> U
    where
        T: Debug + 'static,
    {
        self.any_memo(id, |n| {
            if let Some(n) = n.as_any().downcast_ref::<MemoState<T>>() {
                f(n)
            } else {
                panic!(
                    "couldn't convert {id:?} to MemoState<{}>",
                    std::any::type_name::<T>()
                );
            }
        })
    }

    pub fn any_signal<T>(
        &self,
        id: (ScopeId, SignalId),
        f: impl FnOnce(&Box<dyn AnySignal>) -> T,
    ) -> T {
        self.scope(id.0, |scope| {
            if let Some(n) = scope.signals.borrow().get(id.1) {
                (f)(n)
            } else {
                panic!("couldn't locate {id:?}");
            }
        })
    }

    pub fn signal<T, U>(&self, id: (ScopeId, SignalId), f: impl FnOnce(&SignalState<T>) -> U) -> U
    where
        T: 'static,
    {
        self.any_signal(id, |n| {
            if let Some(n) = n.as_any().downcast_ref::<SignalState<T>>() {
                f(n)
            } else {
                panic!(
                    "couldn't convert {id:?} to SignalState<{}>",
                    std::any::type_name::<T>()
                );
            }
        })
    }

    pub fn running_effect(&self) -> Option<Subscriber> {
        self.stack.borrow().last().cloned()
    }

    pub fn running_transition(&self) -> Option<TransitionState> {
        None // TODO
    }

    pub fn transition(&self) -> Option<TransitionState> {
        None // TODO
    }

    pub fn create_scope(
        &'static self,
        f: impl FnOnce(Scope),
        parent: Option<Scope>,
    ) -> ScopeDisposer {
        let id = { self.scopes.borrow_mut().insert(ScopeState::new(parent)) };
        let scope = Scope { runtime: self, id };
        f(scope);

        ScopeDisposer(Box::new(move || scope.dispose()))
    }

    pub fn push_stack(&self, id: Subscriber) {
        self.stack.borrow_mut().push(id);
    }

    pub fn pop_stack(&self) {
        self.stack.borrow_mut().pop();
    }

    pub fn remove_scope(&self, scope: &ScopeId) {
        let scope = self.scopes.borrow_mut().remove(*scope);
        drop(scope); // unnecessary, but just to be explicit
    }

    pub fn untrack<T>(&self, f: impl FnOnce() -> T) -> T {
        let prev_stack = self.stack.replace(Vec::new());
        let untracked_result = f();
        self.stack.replace(prev_stack);
        untracked_result
    }
}

impl PartialEq for Runtime {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for Runtime {}

impl std::hash::Hash for Runtime {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(&self, state);
    }
}
