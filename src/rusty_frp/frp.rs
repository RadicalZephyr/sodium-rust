use topological_sort::TopologicalSort;
use std::any::Any;
use std::collections::HashMap;
use std::collections::HashSet;
use std::marker::PhantomData;

pub struct FrpContext<ENV> {
    free_cell_id: u32,
    cell_map: HashMap<u32,CellImpl<ENV,Box<Any>>>,
    cells_to_be_updated: HashSet<u32>,
    change_notifiers: Vec<Box<Fn(&mut ENV)>>,
    transaction_depth: u32
}

pub trait WithFrpContext<ENV> {
    fn with_frp_context<F,R>(&self, &mut ENV, k: F) -> R
    where F: FnOnce(&mut FrpContext<ENV>) -> R;
}

impl<ENV: 'static> FrpContext<ENV> {

    pub fn new() -> FrpContext<ENV> {
        FrpContext {
            free_cell_id: 0,
            cell_map: HashMap::new(),
            cells_to_be_updated: HashSet::new(),
            change_notifiers: Vec::new(),
            transaction_depth: 0
        }
    }

    pub fn cell_loop<A,F,F2>(env: &mut ENV, with_frp_context: &F, k:F2) -> Cell<ENV,A>
    where
    A:'static + Clone, // TODO: Eliminate need for Clone.
    F:WithFrpContext<ENV> + Clone + 'static, // TODO: Eliminate need for Clone.
    F2:Fn(&mut ENV,&F,&Cell<ENV,A>)->Cell<ENV,A>
    {
        let mut cell_id: u32 = 0;
        let cell_id2: *mut u32 = &mut cell_id;
        with_frp_context.with_frp_context(
            env,
            move |frp_context| {
                let cell_id = frp_context.free_cell_id;
                frp_context.free_cell_id = frp_context.free_cell_id + 1;
                unsafe { *cell_id2 = cell_id; }
            }
        );
        let cell: Cell<ENV,A> = Cell::of(cell_id);
        let cell2 = k(env,with_frp_context,&Cell::of(cell.id));
        let cell3 = cell2.clone();
        with_frp_context.with_frp_context(
            env,
            move |frp_context| {
                if let Some(cell_impl) = frp_context.cell_map.get_mut(&cell3.id) {
                    cell_impl.id = cell.id;
                }
                let v_op = frp_context.cell_map.remove(&cell3.id);
                match v_op {
                    Some(cell_impl) => {
                        let cell_id = cell.id.clone();
                        frp_context.cell_map.insert(cell_id, cell_impl);
                    }
                    None => ()
                }
            }
        );
        return cell2;
    }

    pub fn new_cell_sink<A,F>(env: &mut ENV, with_frp_context: &F, value: A) -> CellSink<ENV,A>
    where
    A:'static,
    F:WithFrpContext<ENV>
    {
        let mut cell_id: u32 = 0;
        let cell_id2: *mut u32 = &mut cell_id;
        with_frp_context.with_frp_context(
            env,
            move |frp_context| {
                let cell_id = frp_context.free_cell_id;
                frp_context.free_cell_id = frp_context.free_cell_id + 1;
                unsafe {
                    *cell_id2 = cell_id;
                }
                frp_context.cell_map.insert(
                    cell_id.clone(),
                    CellImpl {
                        id: cell_id,
                        free_observer_id: 0,
                        observer_map: HashMap::new(),
                        update_fn_op: None,
                        dependent_cells: Vec::new(),
                        value: Box::new(value) as Box<Any>
                    }
                );
            }
        );
        return CellSink::of(cell_id);
    }

    pub fn map_cell<A,B,CA,F,F2>(env: &mut ENV, with_frp_context: &F, cell: &CA, f: F2) -> Cell<ENV,B>
    where
    A:'static,
    B:'static,
    CA:CellTrait<ENV,A>,
    F:WithFrpContext<ENV>,
    F2:Fn(&A)->B + 'static
    {
        let mut new_cell_id: u32 = 0;
        let new_cell_id2: *mut u32 = &mut new_cell_id;
        let initial_value = f(cell_current_value(cell, env, with_frp_context));
        let cell = Cell::of(cell.id().clone());
        with_frp_context.with_frp_context(
            env,
            move |frp_context| {
                let new_cell_id = frp_context.free_cell_id;
                frp_context.free_cell_id = frp_context.free_cell_id + 1;
                unsafe {
                    *new_cell_id2 = new_cell_id;
                }
                if let Some(cell_impl) = frp_context.cell_map.get_mut(&cell.id) {
                    cell_impl.dependent_cells.push(new_cell_id);
                }
                let update_fn = move |frp_context: &FrpContext<ENV>| {
                    Box::new(f(cell_current_value_via_context(&cell, frp_context))) as Box<Any>
                };
                frp_context.cell_map.insert(
                    new_cell_id.clone(),
                    CellImpl::<ENV,Box<Any>> {
                        id: new_cell_id,
                        free_observer_id: 0,
                        observer_map: HashMap::new(),
                        update_fn_op: Some(Box::new(update_fn)),
                        dependent_cells: Vec::new(),
                        value: Box::new(initial_value) as Box<Any>
                    }
                );
            }
        );
        return Cell::of(new_cell_id);
    }

    pub fn lift2_cell<A,B,C,CA,CB,F,F2>(env: &mut ENV, with_frp_context: &F, f: F2, cell_a: &CA, cell_b: &CB) -> Cell<ENV,C>
    where
    A:'static,
    B:'static,
    C:'static,
    CA: CellTrait<ENV,A>,
    CB: CellTrait<ENV,B>,
    F:WithFrpContext<ENV>,
    F2:Fn(&A,&B)->C + 'static
    {
        let cell_a = Cell::of(cell_a.id().clone());
        let cell_b = Cell::of(cell_b.id().clone());
        let initial_value;
        {
            let value_a = cell_current_value(&cell_a, env, with_frp_context);
            let value_b = cell_current_value(&cell_b, env, with_frp_context);
            initial_value =
                f(
                    value_a, value_b
                );
        }
        let mut new_cell_id: u32 = 0;
        let new_cell_id2: *mut u32 = &mut new_cell_id;
        with_frp_context.with_frp_context(
            env,
            move |frp_context| {
                let new_cell_id = frp_context.free_cell_id;
                frp_context.free_cell_id = frp_context.free_cell_id + 1;
                unsafe {
                    *new_cell_id2 = new_cell_id;
                }
                if let Some(cell_a_impl) = frp_context.cell_map.get_mut(&cell_a.id) {
                    cell_a_impl.dependent_cells.push(new_cell_id);
                }
                if let Some(cell_b_impl) = frp_context.cell_map.get_mut(&cell_b.id) {
                    cell_b_impl.dependent_cells.push(new_cell_id);
                }
                let update_fn = move |frp_context: &FrpContext<ENV>| {
                    Box::new(f(
                        cell_current_value_via_context(&cell_a, frp_context),
                        cell_current_value_via_context(&cell_b, frp_context)
                    )) as Box<Any>
                };
                frp_context.cell_map.insert(
                    new_cell_id.clone(),
                    CellImpl::<ENV,Box<Any>> {
                        id: new_cell_id,
                        free_observer_id: 0,
                        observer_map: HashMap::new(),
                        update_fn_op: Some(Box::new(update_fn)),
                        dependent_cells: Vec::new(),
                        value: Box::new(initial_value) as Box<Any>
                    }
                );
            }
        );
        return Cell::of(new_cell_id);
    }

    pub fn lift3_cell<A,B,C,D,CA,CB,CC,F,F2>(env: &mut ENV, with_frp_context: &F, f: F2, cell_a: &CA, cell_b: &CB, cell_c: &CC) -> Cell<ENV,D>
    where
    A:'static,
    B:'static,
    C:'static,
    D:'static,
    CA:CellTrait<ENV,A>,
    CB:CellTrait<ENV,B>,
    CC:CellTrait<ENV,C>,
    F:WithFrpContext<ENV>,
    F2:Fn(&A,&B,&C)->D + 'static
    {
        let cell_a = Cell::of(cell_a.id().clone());
        let cell_b = Cell::of(cell_b.id().clone());
        let cell_c = Cell::of(cell_c.id().clone());
        let initial_value;
        {
            let value_a = cell_current_value(&cell_a, env, with_frp_context);
            let value_b = cell_current_value(&cell_b, env, with_frp_context);
            let value_c = cell_current_value(&cell_c, env, with_frp_context);
            initial_value =
                f(
                    value_a, value_b, value_c
                );
        }
        let mut new_cell_id: u32 = 0;
        let new_cell_id2: *mut u32 = &mut new_cell_id;
        with_frp_context.with_frp_context(
            env,
            move |frp_context| {
                let new_cell_id = frp_context.free_cell_id;
                frp_context.free_cell_id = frp_context.free_cell_id + 1;
                unsafe {
                    *new_cell_id2 = new_cell_id;
                }
                if let Some(cell_a_impl) = frp_context.cell_map.get_mut(&cell_a.id) {
                    cell_a_impl.dependent_cells.push(new_cell_id);
                }
                if let Some(cell_b_impl) = frp_context.cell_map.get_mut(&cell_b.id) {
                    cell_b_impl.dependent_cells.push(new_cell_id);
                }
                if let Some(cell_c_impl) = frp_context.cell_map.get_mut(&cell_c.id) {
                    cell_c_impl.dependent_cells.push(new_cell_id);
                }
                let update_fn = move |frp_context: &FrpContext<ENV>| {
                    Box::new(f(
                        cell_current_value_via_context(&cell_a, frp_context),
                        cell_current_value_via_context(&cell_b, frp_context),
                        cell_current_value_via_context(&cell_c, frp_context)
                    )) as Box<Any>
                };
                frp_context.cell_map.insert(
                    new_cell_id.clone(),
                    CellImpl::<ENV,Box<Any>> {
                        id: new_cell_id,
                        free_observer_id: 0,
                        observer_map: HashMap::new(),
                        update_fn_op: Some(Box::new(update_fn)),
                        dependent_cells: Vec::new(),
                        value: Box::new(initial_value) as Box<Any>
                    }
                );
            }
        );
        return Cell::of(new_cell_id);
    }

    pub fn lift4_cell<A,B,C,D,E,CA,CB,CC,CD,F,F2>(env: &mut ENV, with_frp_context: &F, f: F2, cell_a: &CA, cell_b: &CB, cell_c: &CC, cell_d: &CD) -> Cell<ENV,E>
    where
    A:'static,
    B:'static,
    C:'static,
    D:'static,
    E:'static,
    CA: CellTrait<ENV,A>,
    CB: CellTrait<ENV,B>,
    CC: CellTrait<ENV,C>,
    CD: CellTrait<ENV,D>,
    F:WithFrpContext<ENV>,
    F2:Fn(&A,&B,&C,&D)->E + 'static
    {
        let cell_a = Cell::of(cell_a.id().clone());
        let cell_b = Cell::of(cell_b.id().clone());
        let cell_c = Cell::of(cell_c.id().clone());
        let cell_d = Cell::of(cell_d.id().clone());
        let initial_value;
        {
            let value_a = cell_current_value(&cell_a, env, with_frp_context);
            let value_b = cell_current_value(&cell_b, env, with_frp_context);
            let value_c = cell_current_value(&cell_c, env, with_frp_context);
            let value_d = cell_current_value(&cell_d, env, with_frp_context);
            initial_value =
                f(
                    value_a, value_b, value_c, value_d
                );
        }
        let mut new_cell_id: u32 = 0;
        let new_cell_id2: *mut u32 = &mut new_cell_id;
        with_frp_context.with_frp_context(
            env,
            move |frp_context| {
                let new_cell_id = frp_context.free_cell_id;
                frp_context.free_cell_id = frp_context.free_cell_id + 1;
                unsafe {
                    *new_cell_id2 = new_cell_id;
                }
                if let Some(cell_a_impl) = frp_context.cell_map.get_mut(&cell_a.id) {
                    cell_a_impl.dependent_cells.push(new_cell_id);
                }
                if let Some(cell_b_impl) = frp_context.cell_map.get_mut(&cell_b.id) {
                    cell_b_impl.dependent_cells.push(new_cell_id);
                }
                if let Some(cell_c_impl) = frp_context.cell_map.get_mut(&cell_c.id) {
                    cell_c_impl.dependent_cells.push(new_cell_id);
                }
                if let Some(cell_d_impl) = frp_context.cell_map.get_mut(&cell_d.id) {
                    cell_d_impl.dependent_cells.push(new_cell_id);
                }
                let update_fn = move |frp_context: &FrpContext<ENV>| {
                    Box::new(f(
                        cell_current_value_via_context(&cell_a, frp_context),
                        cell_current_value_via_context(&cell_b, frp_context),
                        cell_current_value_via_context(&cell_c, frp_context),
                        cell_current_value_via_context(&cell_d, frp_context)
                    )) as Box<Any>
                };
                frp_context.cell_map.insert(
                    new_cell_id.clone(),
                    CellImpl::<ENV,Box<Any>> {
                        id: new_cell_id,
                        free_observer_id: 0,
                        observer_map: HashMap::new(),
                        update_fn_op: Some(Box::new(update_fn)),
                        dependent_cells: Vec::new(),
                        value: Box::new(initial_value) as Box<Any>
                    }
                );
            }
        );
        return Cell::of(new_cell_id);
    }

    pub fn transaction<F,F2>(env: &mut ENV, with_frp_context: &F, k: F2)
    where
    F:WithFrpContext<ENV>, F2: FnOnce(&mut ENV, &F),
    {
        with_frp_context.with_frp_context(
            env,
            |frp_context| {
                frp_context.transaction_depth = frp_context.transaction_depth + 1;
            }
        );
        k(env, with_frp_context);
        let final_transaction_depth = with_frp_context.with_frp_context(
            env,
            |frp_context| {
                frp_context.transaction_depth = frp_context.transaction_depth - 1;
                frp_context.transaction_depth
            }
        );
        if final_transaction_depth == 0 {
            FrpContext::propergate(env, with_frp_context);
        }
    }

    fn propergate<F>(env: &mut ENV, with_frp_context: &F)
    where F:WithFrpContext<ENV>
    {
        let mut ts = TopologicalSort::<u32>::new();
        let mut change_notifiers: Vec<Box<Fn(&mut ENV)>> = Vec::new();
        let change_notifiers2: *mut Vec<Box<Fn(&mut ENV)>> = &mut change_notifiers;
        with_frp_context.with_frp_context(
            env,
            move |frp_context| {
                frp_context.transaction_depth = frp_context.transaction_depth + 1;
                for cell_to_be_updated in &frp_context.cells_to_be_updated {
                    ts.insert(cell_to_be_updated.clone());
                    if let &Some(cell) = &frp_context.cell_map.get(cell_to_be_updated) {
                        for dependent_cell in &cell.dependent_cells {
                            ts.add_dependency(cell.id, dependent_cell.clone());
                        }
                    }
                }
                loop {
                    let next_op = ts.pop();
                    match next_op {
                        Some(cell_id) => {
                            frp_context.update_cell(&cell_id);
                        },
                        None => break
                    }
                }
                frp_context.transaction_depth = frp_context.transaction_depth - 1;
                unsafe { (*change_notifiers2).append(&mut frp_context.change_notifiers) };
            }
        );
        for change_notifier in change_notifiers {
            change_notifier(env);
        }
    }

    fn update_cell(&mut self, cell_id: &u32)
    {
        let value;
        if let Some(cell) = self.cell_map.get(cell_id) {
            match &cell.update_fn_op {
                &Some(ref update_fn) => {
                    value = update_fn(self);
                },
                &None => return
            }
        } else {
            return;
        }
        let mut notifiers_to_add: Vec<Box<Fn(&mut ENV)>> = Vec::new();
        if let Some(cell) = self.cell_map.get_mut(cell_id) {
            cell.value = value;
            let cell2: *const CellImpl<ENV,Box<Any>> = cell;
            notifiers_to_add.push(Box::new(
                move |env| {
                    unsafe {
                        let ref cell3: CellImpl<ENV,Box<Any>> = *cell2;
                        for observer in cell3.observer_map.values() {
                            observer(env, &cell3.value);
                        }
                    }
                }
            ));
        }
        self.change_notifiers.append(&mut notifiers_to_add);
    }

    fn mark_all_decendent_cells_for_update(&mut self, cell_id: u32, visited: &mut HashSet<u32>) {
        visited.insert(cell_id);
        let mut dependent_cells: Vec<u32> = Vec::new();
        match self.cell_map.get(&cell_id) {
            Some(cell) => {
                for dependent_cell in &cell.dependent_cells {
                    dependent_cells.push(dependent_cell.clone());
                }
            },
            None => ()
        }
        loop {
            let dependent_cell_op = dependent_cells.pop();
            match dependent_cell_op {
                Some(dependent_cell) => {
                    if !visited.contains(&dependent_cell) {
                        self.cells_to_be_updated.insert(dependent_cell);
                        self.mark_all_decendent_cells_for_update(dependent_cell, visited);
                    }
                },
                None => break
            }
        }
    }
}

pub trait CellTrait<ENV:'static,A:'static>: Sized {
    fn id(&self) -> u32;

    fn current_value<'a,F>(&self, env: &'a mut ENV, with_frp_context: &F) -> &'a A
    where
    F:WithFrpContext<ENV>
    {
        cell_current_value(self, env, with_frp_context)
    }

    fn observe<F,F2>(&self, env: &mut ENV, with_frp_context: &F, observer: F2) -> Box<FnOnce(&mut ENV, &F)>
    where
    F:WithFrpContext<ENV>,
    F2:Fn(&mut ENV,&A) + 'static
    {
        {
            let env2: *mut ENV = env;
            let value = self.current_value(unsafe { &mut *env2 }, with_frp_context);
            let value2: *const A = value;
            observer(unsafe { &mut *env2 }, unsafe { &*value2 });
        }
        let mut observer_id_op: Option<u32> = None;
        let observer_id_op2: *mut Option<u32> = &mut observer_id_op;
        let cell_id = self.id().clone();
        with_frp_context.with_frp_context(
            env,
            move |frp_context| {
                if let Some(cell) = frp_context.cell_map.get_mut(&cell_id) {
                    let observer_id = cell.free_observer_id;
                    unsafe { *observer_id_op2 = Some(observer_id); }
                    cell.free_observer_id = cell.free_observer_id + 1;
                    cell.observer_map.insert(observer_id, Box::new(
                        move |env, value| {
                            match value.as_ref().downcast_ref::<A>() {
                                Some(value) => observer(env, value),
                                None => ()
                            }
                        }
                    ));
                }
            }
        );
        let cell_id = self.id().clone();
        match observer_id_op {
            Some(observer_id) => {
                return Box::new(move |env, with_frp_context| {
                    with_frp_context.with_frp_context(
                        env,
                        move |frp_context| {
                            if let Some(cell) = frp_context.cell_map.get_mut(&cell_id) {
                                cell.observer_map.remove(&observer_id);
                            }
                        }
                    );
                });
            },
            None => Box::new(|_, _| {})
        }
    }
}

// NOTE: Not safe for API use. Internal use only!
fn cell_current_value<ENV:'static,A:'static,C,F>(cell: &C, env: &mut ENV, with_frp_context: &F) -> &'static A
where
C: CellTrait<ENV,A>,
F:WithFrpContext<ENV>
{
    let mut value_op: Option<*const A> = None;
    let value_op2: *mut Option<*const A> = &mut value_op;
    with_frp_context.with_frp_context(
        env,
        move |frp_context: &mut FrpContext<ENV>| {
            let value = cell_current_value_via_context(cell, frp_context);
            unsafe { (*value_op2) = Some(value); }
        }
    );
    match value_op {
        Some(value) => {
            unsafe { &*value }
        },
        None => panic!("")
    }
}

// NOTE: Not safe for API use. Internal use only!
fn cell_current_value_via_context<ENV:'static,A:'static,C>(cell: &C, frp_context: &FrpContext<ENV>) -> &'static A
where
C: CellTrait<ENV,A>
{
    let result: *const A;
    match frp_context.cell_map.get(&cell.id()) {
        Some(cell) => {
            match cell.value.as_ref().downcast_ref::<A>() {
                Some(value) => result = value,
                None => panic!("")
            }
        },
        None => panic!("")
    }
    return unsafe { &*result };
}

pub struct Cell<ENV,A> {
    id: u32,
    env_phantom: PhantomData<ENV>,
    value_phantom: PhantomData<A>
}

impl<ENV:'static,A:'static> Clone for Cell<ENV,A> {
    fn clone(&self) -> Self {
        Cell::of(self.id.clone())
    }
}

impl<ENV:'static,A:'static> Copy for Cell<ENV,A> {}

impl<ENV:'static,A:'static> CellTrait<ENV,A> for Cell<ENV,A> {
    fn id(&self) -> u32 {
        self.id
    }
}

impl<ENV,A> Cell<ENV,A> {
    fn of(id: u32) -> Cell<ENV,A> {
        Cell {
            id: id,
            env_phantom: PhantomData,
            value_phantom: PhantomData
        }
    }
}

pub struct CellSink<ENV,A> {
    id: u32,
    env_phantom: PhantomData<ENV>,
    value_phantom: PhantomData<A>
}

impl<ENV:'static,A:'static> Clone for CellSink<ENV,A> {
    fn clone(&self) -> Self {
        CellSink::of(self.id.clone())
    }
}

impl<ENV:'static,A:'static> Copy for CellSink<ENV,A> {}

impl<ENV:'static,A:'static> CellTrait<ENV,A> for CellSink<ENV,A> {
    fn id(&self) -> u32 {
        self.id
    }
}

impl<ENV:'static,A:'static> CellSink<ENV,A> {
    fn of(id: u32) -> CellSink<ENV,A> {
        CellSink {
            id: id,
            env_phantom: PhantomData,
            value_phantom: PhantomData
        }
    }

    pub fn change_value<F>(&self, env: &mut ENV, with_frp_context: &F, value: A)
    where F:WithFrpContext<ENV> {
        let cell_id = self.id.clone();
        FrpContext::transaction(
            env,
            with_frp_context,
            move |env, with_frp_context| {
                with_frp_context.with_frp_context(
                    env,
                    move |frp_context| {
                        if let Some(cell) = frp_context.cell_map.get_mut(&cell_id) {
                            cell.value = Box::new(value) as Box<Any>;
                        }
                        frp_context.mark_all_decendent_cells_for_update(cell_id, &mut HashSet::new());
                    }
                );
            }
        );
    }
}

struct CellImpl<ENV,A> {
    id: u32,
    free_observer_id: u32,
    observer_map: HashMap<u32,Box<Fn(&mut ENV,&A)>>,
    update_fn_op: Option<Box<Fn(&FrpContext<ENV>)->A>>,
    dependent_cells: Vec<u32>,
    value: A
}
