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

pub struct Node<T> {
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
    pub fn new() -> LruTable<T> {
        LruTable {
            table: HashMap::new(),
            head: None,
            tail: None
        }
    }

    pub fn get(&mut self, key: &<T as LruItem>::Key) -> Option<&mut T> {
        let mut node = self.table.get_mut(key);
        if let Some(node) = &mut node {
            LruTable::dettach(node.as_mut(), &mut self.head, &mut self.tail);
            LruTable::attach_front(node.as_mut(), &mut self.head, &mut self.tail);
        }
        node.map(|n| &mut n.value)
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

        LruTable::attach_front(node.as_mut(), &mut self.head, &mut self.tail);
        previous
    }

    pub fn pop_back(&mut self) -> Option<T> {
        self.tail.map(|node| unsafe {
            let key = node.as_ref().value.key_ref();
            let mut node = self.table.remove(key).unwrap_unchecked();
            LruTable::dettach(&mut node, &mut self.head, &mut self.tail);
            node.value
        })
    }

    fn attach_front(node: &mut Node<T>, head: &mut Option<NonNull<Node<T>>>, tail: &mut Option<NonNull<Node<T>>>) {
        node.prev = None;
        node.next = *head;

        let node_ptr = NonNull::new(node).unwrap();
        if let Some(head) = head {
            unsafe { head.as_mut().prev = Some(node_ptr) };
        }
        *head = Some(node_ptr);

        if tail.is_none() {
            *tail = Some(node_ptr);
        }
    }

    fn dettach(node: &mut Node<T>, head: &mut Option<NonNull<Node<T>>>, tail: &mut Option<NonNull<Node<T>>>) {
        if let Some(mut prev) = node.prev {
            unsafe { prev.as_mut().next = node.next }
        } else {
            // is in head
            *head = node.next
        }
        if let Some(mut next) = node.next {
            unsafe { next.as_mut().prev = node.prev }
        } else {
            // is in tail
            *tail = node.prev
        }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    impl LruItem for i32 {
        type Key = i32;
        fn key(&self) -> Self::Key { *self }
        fn key_ref(&self) -> &Self::Key { self }
    }

    #[test]
    fn test() {
        let mut table = LruTable::new();
        table.push_front(1);
        table.push_front(2);
        table.push_front(3);
        table.push_front(4);
        table.push_front(5);
        table.push_front(6); // 6, 5, 4, 3, 2, 1

        table.get(&5); // 5, 6, 4, 3, 2, 1
        table.get(&2); // 2, 5, 6, 4, 3, 1
        table.get(&1); // 1, 2, 5, 6, 4, 3

        assert_eq!(table.pop_back(), Some(3));
        assert_eq!(table.pop_back(), Some(4));
        assert_eq!(table.pop_back(), Some(6));
        assert_eq!(table.pop_back(), Some(5));
        assert_eq!(table.pop_back(), Some(2));
        assert_eq!(table.pop_back(), Some(1));
        assert_eq!(table.pop_back(), None);
    }
}