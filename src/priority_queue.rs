use std::collections::BinaryHeap;

#[derive(Debug)]
struct HeapValue<K, V>(K, V);

impl<K: Ord, V> Ord for HeapValue<K, V> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<K: Ord, V> PartialOrd for HeapValue<K, V> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: Ord, V> PartialEq for HeapValue<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.0.cmp(&other.0).is_eq()
    }
}

impl<K: Ord, V> Eq for HeapValue<K, V> {}

#[derive(Debug)]
pub struct PriorityQueue<K, V>(BinaryHeap<HeapValue<K, V>>);

impl<K: Ord, V> PriorityQueue<K, V> {
    pub fn new() -> Self {
        PriorityQueue(BinaryHeap::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        PriorityQueue(BinaryHeap::with_capacity(capacity))
    }

    pub fn push(&mut self, k: K, v: V) {
        self.0.push(HeapValue(k, v))
    }

    pub fn pop(&mut self) -> Option<(K, V)> {
        self.0.pop().map(|HeapValue(k, v)| (k, v))
    }

    pub fn peek(&self) -> Option<(&K, &V)> {
        self.0.peek().map(|HeapValue(k, v)| (k, v))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter().map(|HeapValue(k, v)| (k, v))
    }

    pub fn into_iter(self) -> impl Iterator<Item = (K, V)> {
        self.0.into_iter().map(|HeapValue(k, v)| (k, v))
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }
}

impl<K: Ord, V> Default for PriorityQueue<K, V> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<K: Ord, V, const N: usize> From<[(K, V); N]> for PriorityQueue<K, V> {
    fn from(arr: [(K, V); N]) -> Self {
        Self(BinaryHeap::from_iter(arr.map(|(k, v)| HeapValue(k, v))))
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for PriorityQueue<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut c = PriorityQueue::new();

        for (k, v) in iter {
            c.push(k, v);
        }

        c
    }
}
