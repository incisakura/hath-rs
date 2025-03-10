use std::collections::HashMap;
use std::hash::Hash;
use std::mem;
use std::ptr::NonNull;

pub trait LruItem {
    type Key;

    fn key(&self) -> Self::Key;

    fn key_ref(&self) -> &Self::Key;
}

pub struct LruTable<T: LruItem> {
    table: HashMap<<T as LruItem>::Key, Box<Node<T>>>,
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
}

struct Node<T> {
    value: T,
    prev: Option<NonNull<Node<T>>>,
    next: Option<NonNull<Node<T>>>,
}

impl<T> Node<T> {
    pub fn new(value: T) -> Node<T> {
        Node {
            value,
            prev: None,
            next: None,
        }
    }
}

impl<T> LruTable<T>
where
    T: LruItem,
    <T as LruItem>::Key: Eq + Hash,
{
    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn get(&mut self, key: &<T as LruItem>::Key) -> Option<&T> {
        self.table.get_mut(key).map(|node| {
            unsafe { LruTable::move_front(node, &mut self.head) };
            &node.value
        })
    }

    pub fn push_front(&mut self, value: T) -> Option<T> {
        use std::collections::hash_map::Entry;

        let mut previous = None;
        let node = match self.table.entry(value.key()) {
            Entry::Occupied(mut entry) => {
                // replace existed value to avoid alloc
                previous = Some(mem::replace(&mut entry.get_mut().value, value));
                entry.into_mut()
            }
            Entry::Vacant(entry) => entry.insert(Box::new(Node::new(value))),
        };
        unsafe { LruTable::move_front(node, &mut self.head) };
        previous
    }

    pub fn pop_back(&mut self) -> Option<T> {
        self.tail.map(|node| unsafe {
            let key = node.as_ref().value.key_ref();
            let node = self.table.remove(key).unwrap_unchecked();

            self.tail = node.prev;
            if let Some(mut prev) = node.prev {
                prev.as_mut().next = None;
            }

            node.value
        })
    }

    unsafe fn move_front(node: &mut Node<T>, head: &mut Option<NonNull<Node<T>>>) {
        let node_ptr = Some(NonNull::new(node).unwrap());
        if let Some(mut head) = head {
            head.as_mut().prev = node_ptr;
        }
        if let Some(mut prev) = node.prev {
            prev.as_mut().next = node.next;
        }
        if let Some(mut next) = node.next {
            next.as_mut().prev = node.prev;
        }
        *head = node_ptr;
    }
}
