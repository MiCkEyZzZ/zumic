//! R-tree spatial index для эффективного поиска географических точек
//!
//! Реализует R-tree структуру данных для O(log n) radius queries вместо O(n).
//! Поддерживает динамические updated, k-NN search, и range queries.

use std::{cmp::Ordering, collections::BinaryHeap, f64};

use crate::{database::haversine_distance, GeoEntry, GeoPoint};

/// Минимальное количество элементов в узле (обычно M/2).
#[allow(dead_code)]
const MIN_ENTRIES: usize = 2;
/// Максимальное количество элементов в узле.
const MAX_ENTRIES: usize = 8;

/// Узел R-tree (внутренний или листовой).
#[allow(clippy::vec_box)]
#[derive(Debug, Clone)]
enum RTreeNode {
    Leaf {
        entries: Vec<GeoEntry>,
        bbox: BoundingBox,
    },
    Internal {
        children: Vec<Box<RTreeNode>>,
        bbox: BoundingBox,
    },
}

/// Прямоугольная область (bounding box) на карте.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min_lon: f64,
    pub max_lon: f64,
    pub min_lat: f64,
    pub max_lat: f64,
}

/// R-tree для spatial indexing географических точек.
#[derive(Debug)]
pub struct RTree {
    root: Option<Box<RTreeNode>>,
    size: usize,
}

/// Статистика R-tree.
#[derive(Debug, Clone, Copy)]
pub struct TreeStats {
    pub depth: usize,
    pub node_count: usize,
    pub leaf_count: usize,
}

/// Элемент priority queue для k-NN search.
#[derive(Debug)]
struct PqItem<'a> {
    dist: f64,
    node: &'a RTreeNode,
    entry: Option<&'a GeoEntry>,
}

/// Элемент результата для k-NN (max-heap по дистанции).
#[derive(Debug, Clone)]
struct ResultItem {
    entry: GeoEntry,
    dist: f64,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl BoundingBox {
    /// Создаёт bounding box из одной точки.
    pub fn from_point(point: GeoPoint) -> Self {
        Self {
            min_lon: point.lon,
            max_lon: point.lon,
            min_lat: point.lat,
            max_lat: point.lat,
        }
    }

    /// Создаёт bounding box из двух углов.
    pub fn new(
        min_lon: f64,
        max_lon: f64,
        min_lat: f64,
        max_lat: f64,
    ) -> Self {
        Self {
            min_lon,
            max_lon,
            min_lat,
            max_lat,
        }
    }

    /// Расширяет bbox, чтобы включить другой bbox.
    pub fn expand(
        &mut self,
        other: &BoundingBox,
    ) {
        self.min_lon = self.min_lon.min(other.min_lon);
        self.max_lon = self.max_lon.max(other.max_lon);
        self.min_lat = self.min_lat.min(other.min_lat);
        self.max_lat = self.max_lat.max(other.max_lat);
    }

    /// Вычисляет площадь bounding box (в градусах²).
    pub fn area(&self) -> f64 {
        (self.max_lon - self.min_lon) * (self.max_lat - self.min_lat)
    }

    /// Проверяет, пересекаются ли два bbox.
    pub fn intersects(
        &self,
        other: &BoundingBox,
    ) -> bool {
        self.min_lon <= other.max_lon
            && self.max_lon >= other.min_lon
            && self.min_lat <= other.max_lat
            && self.max_lat >= other.min_lat
    }

    /// Проверяет, содержит ли bbox точку.
    pub fn contains_point(
        &self,
        point: GeoPoint,
    ) -> bool {
        point.lon >= self.min_lon
            && point.lon <= self.max_lon
            && point.lat >= self.min_lat
            && point.lat <= self.max_lat
    }

    /// Вычисляет минимальное расстояние от точки до bbox (в градусах).
    pub fn min_distance_to_point(
        &self,
        point: GeoPoint,
    ) -> f64 {
        let dx = if point.lon < self.min_lon {
            self.min_lon - point.lon
        } else if point.lon > self.max_lon {
            point.lon - self.max_lon
        } else {
            0.0
        };

        let dy = if point.lat < self.min_lat {
            self.min_lat - point.lat
        } else if point.lat > self.max_lat {
            point.lat - self.max_lat
        } else {
            0.0
        };

        (dx * dx + dy * dy).sqrt()
    }

    /// Вычисляет центр bbox.
    pub fn center(&self) -> GeoPoint {
        GeoPoint {
            lon: (self.min_lon + self.max_lon) * 0.5,
            lat: (self.min_lat + self.max_lat) * 0.5,
        }
    }
}

impl RTreeNode {
    /// Создаёт пустой листовой узел.
    fn new_leaf() -> Self {
        RTreeNode::Leaf {
            entries: Vec::with_capacity(MAX_ENTRIES),
            bbox: BoundingBox::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Создаёт пустой внутренний узел.
    fn new_internal() -> Self {
        RTreeNode::Internal {
            children: Vec::with_capacity(MAX_ENTRIES),
            bbox: BoundingBox::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Возвращает bounding box узла.
    fn bbox(&self) -> &BoundingBox {
        match self {
            RTreeNode::Leaf { bbox, .. } => bbox,
            RTreeNode::Internal { bbox, .. } => bbox,
        }
    }

    /// Является ли узел листом.
    #[allow(dead_code)]
    fn is_leaf(&self) -> bool {
        matches!(self, RTreeNode::Leaf { .. })
    }

    /// Обновляет bounding box узла на основе дочерних элементов.
    fn update_bbox(&mut self) {
        match self {
            RTreeNode::Leaf { entries, bbox } => {
                if entries.is_empty() {
                    *bbox = BoundingBox::new(0.0, 0.0, 0.0, 0.0);
                } else {
                    *bbox = BoundingBox::from_point(entries[0].point);
                    for entry in entries.iter().skip(1) {
                        bbox.expand(&BoundingBox::from_point(entry.point));
                    }
                }
            }
            RTreeNode::Internal { children, bbox } => {
                if children.is_empty() {
                    *bbox = BoundingBox::new(0.0, 0.0, 0.0, 0.0);
                } else {
                    *bbox = *children[0].bbox();
                    for child in children.iter().skip(1) {
                        bbox.expand(child.bbox());
                    }
                }
            }
        }
    }
}

impl RTree {
    /// Создаёт пустое дерево.
    pub fn new() -> Self {
        Self {
            root: None,
            size: 0,
        }
    }

    /// Bulk loading: строит дерево из отсортированных элементов.
    /// Эффективнее последовательных insert для больших datasets.
    pub fn bulk_load(mut entries: Vec<GeoEntry>) -> Self {
        if entries.is_empty() {
            return Self::new();
        }

        let size = entries.len();

        // Sort-Tile-Recursive (STR) algorithm
        // Сортируем по lon, затем группируем и сортируем по lat
        entries.sort_by(|a, b| {
            a.point
                .lon
                .partial_cmp(&b.point.lon)
                .unwrap_or(Ordering::Equal)
        });

        let root = Self::build_str_tree(entries, 0);

        Self {
            root: Some(root),
            size,
        }
    }

    /// Рекурсивное построение дерева методом STR.
    fn build_str_tree(
        entries: Vec<GeoEntry>,
        _level: usize,
    ) -> Box<RTreeNode> {
        if entries.len() <= MAX_ENTRIES {
            // Листовой узел
            let mut node = RTreeNode::new_leaf();
            if let RTreeNode::Leaf {
                entries: ref mut e, ..
            } = node
            {
                *e = entries;
            }
            node.update_bbox();
            return Box::new(node);
        }

        // Разбиваем на страйпы по latitude
        let slice_count = ((entries.len() as f64).sqrt().ceil() as usize).max(2);
        let slice_size = entries.len().div_ceil(slice_count);

        let mut slices: Vec<Vec<GeoEntry>> = Vec::new();
        let mut current_slice = Vec::new();

        for entry in entries {
            current_slice.push(entry);
            if current_slice.len() >= slice_size {
                slices.push(current_slice);
                current_slice = Vec::new();
            }
        }
        if !current_slice.is_empty() {
            slices.push(current_slice);
        }

        // Сортируем каждый страйп по lat
        for slice in &mut slices {
            slice.sort_by(|a, b| {
                a.point
                    .lat
                    .partial_cmp(&b.point.lat)
                    .unwrap_or(Ordering::Equal)
            });
        }

        // Создаём дочерние узлы
        let mut children = Vec::new();
        for slice in slices {
            children.push(Self::build_str_tree(slice, _level + 1));
        }

        let mut node = RTreeNode::new_internal();
        if let RTreeNode::Internal {
            children: ref mut c,
            ..
        } = node
        {
            *c = children;
        }
        node.update_bbox();
        Box::new(node)
    }

    /// Вставляет элемент в дерево.
    pub fn insert(
        &mut self,
        entry: GeoEntry,
    ) {
        if self.root.is_none() {
            let mut leaf = RTreeNode::new_leaf();
            if let RTreeNode::Leaf {
                entries: ref mut e, ..
            } = leaf
            {
                e.push(entry);
            }
            leaf.update_bbox();
            self.root = Some(Box::new(leaf));
            self.size = 1;
            return;
        }

        let split_node = {
            let root = self.root.as_mut().unwrap();
            Self::insert_recursive(root, entry)
        };

        if let Some(new_node) = split_node {
            // Root split: создаём новый root
            let old_root = self.root.take().unwrap();
            let mut new_root = RTreeNode::new_internal();
            if let RTreeNode::Internal {
                children: ref mut c,
                ..
            } = new_root
            {
                c.push(old_root);
                c.push(new_node);
            }
            new_root.update_bbox();
            self.root = Some(Box::new(new_root));
        }

        self.size += 1;
    }

    /// Рекурсивная вставка с возможным split.
    fn insert_recursive(
        node: &mut RTreeNode,
        entry: GeoEntry,
    ) -> Option<Box<RTreeNode>> {
        use std::mem;

        match node {
            // ---- LEAF ----
            RTreeNode::Leaf { .. } => {
                // Берём ownership над вектором entries
                let mut all_entries = if let RTreeNode::Leaf { entries, .. } = node {
                    mem::take(entries)
                } else {
                    unreachable!()
                };

                // Вставляем новую запись
                all_entries.push(entry);

                // Если не нужно делить — просто возвращаем обратно и обновляем bbox
                if all_entries.len() <= MAX_ENTRIES {
                    if let RTreeNode::Leaf { entries, .. } = node {
                        *entries = all_entries;
                    }
                    node.update_bbox();
                    return None;
                }

                // Выполняем split (выбираем seeds и распределяем оставшиеся)
                let (seed1, seed2) = Self::pick_seeds_leaf(&all_entries);

                // Удаляем seed'ы из all_entries, удаляем больший индекс первым
                let (first_seed, second_seed) = if seed1 > seed2 {
                    (all_entries.remove(seed1), all_entries.remove(seed2))
                } else {
                    (all_entries.remove(seed2), all_entries.remove(seed1))
                };

                let mut group1: Vec<GeoEntry> = vec![first_seed];
                let mut group2: Vec<GeoEntry> = vec![second_seed];

                for e in all_entries.into_iter() {
                    // вычисляем bbox для групп
                    let mut bbox1 = BoundingBox::from_point(group1[0].point);
                    for ge in &group1 {
                        bbox1.expand(&BoundingBox::from_point(ge.point));
                    }

                    let mut bbox2 = BoundingBox::from_point(group2[0].point);
                    for ge in &group2 {
                        bbox2.expand(&BoundingBox::from_point(ge.point));
                    }

                    let mut expanded1 = bbox1;
                    expanded1.expand(&BoundingBox::from_point(e.point));
                    let mut expanded2 = bbox2;
                    expanded2.expand(&BoundingBox::from_point(e.point));

                    let enlargement1 = expanded1.area() - bbox1.area();
                    let enlargement2 = expanded2.area() - bbox2.area();

                    if enlargement1 < enlargement2 {
                        group1.push(e);
                    } else {
                        group2.push(e);
                    }
                }

                // Положим первую группу обратно в узел и создадим новый узел для второй
                if let RTreeNode::Leaf { entries, .. } = node {
                    *entries = group1;
                }
                node.update_bbox();

                let mut new_node = RTreeNode::new_leaf();
                if let RTreeNode::Leaf {
                    entries: ref mut e, ..
                } = new_node
                {
                    *e = group2;
                }
                new_node.update_bbox();

                Some(Box::new(new_node))
            }

            // ---- INTERNAL ----
            RTreeNode::Internal { children, .. } => {
                // Сначала вычисляем лучший ребёнок по текущим children (пока они ещё доступны)
                let best_idx = Self::choose_subtree(children, &entry);

                // Берём владение над вектором children
                let mut old_children = if let RTreeNode::Internal { children, .. } = node {
                    mem::take(children)
                } else {
                    unreachable!()
                };

                // Рекурсивно вставляем в выбранного ребёнка (old_children — owned)
                let split_child = Self::insert_recursive(&mut old_children[best_idx], entry);

                if let Some(new_child) = split_child {
                    old_children.push(new_child);
                }

                // Если не требуется split — записываем обратно и обновляем bbox
                if old_children.len() <= MAX_ENTRIES {
                    if let RTreeNode::Internal { children, .. } = node {
                        *children = old_children;
                    }
                    node.update_bbox();
                    return None;
                }

                // Split internal: извлекаем два seed'а (удаляем больший индекс первым)
                let (seed1, seed2) = Self::pick_seeds_internal(&old_children);
                let (first_child, second_child) = if seed1 > seed2 {
                    (old_children.remove(seed1), old_children.remove(seed2))
                } else {
                    (old_children.remove(seed2), old_children.remove(seed1))
                };

                let mut group1: Vec<Box<RTreeNode>> = vec![first_child];
                let mut group2: Vec<Box<RTreeNode>> = vec![second_child];

                for child in old_children.into_iter() {
                    let mut bbox1 = *group1[0].bbox();
                    for c in &group1 {
                        bbox1.expand(c.bbox());
                    }

                    let mut bbox2 = *group2[0].bbox();
                    for c in &group2 {
                        bbox2.expand(c.bbox());
                    }

                    let mut expanded1 = bbox1;
                    expanded1.expand(child.bbox());
                    let mut expanded2 = bbox2;
                    expanded2.expand(child.bbox());

                    let enlargement1 = expanded1.area() - bbox1.area();
                    let enlargement2 = expanded2.area() - bbox2.area();

                    if enlargement1 < enlargement2 {
                        group1.push(child);
                    } else {
                        group2.push(child);
                    }
                }

                // Положим первую группу обратно и создаём новый узел для второй
                if let RTreeNode::Internal { children, .. } = node {
                    *children = group1;
                }
                node.update_bbox();

                let mut new_node = RTreeNode::new_internal();
                if let RTreeNode::Internal {
                    children: ref mut c,
                    ..
                } = new_node
                {
                    *c = group2;
                }
                new_node.update_bbox();

                Some(Box::new(new_node))
            }
        }
    }

    /// Выбирает лучший дочерний узел для вставки (минимальное увеличение
    /// площади).
    fn choose_subtree(
        children: &[Box<RTreeNode>],
        entry: &GeoEntry,
    ) -> usize {
        let entry_bbox = BoundingBox::from_point(entry.point);
        let mut best_idx = 0;
        let mut min_enlargement = f64::INFINITY;

        for (i, child) in children.iter().enumerate() {
            let mut expanded = *child.bbox();
            expanded.expand(&entry_bbox);
            let enlargement = expanded.area() - child.bbox().area();

            if enlargement < min_enlargement {
                min_enlargement = enlargement;
                best_idx = i;
            }
        }

        best_idx
    }

    /// Split листового узла на два.
    #[allow(dead_code)]
    fn split_leaf(node: &mut RTreeNode) -> Box<RTreeNode> {
        if let RTreeNode::Leaf { entries, .. } = node {
            // Quadratic split: выбираем две самые далёкие точки
            let (seed1, seed2) = Self::pick_seeds_leaf(entries);

            let mut group1 = vec![entries[seed1].clone()];
            let mut group2 = vec![entries[seed2].clone()];

            for (i, entry) in entries.iter().enumerate() {
                if i == seed1 || i == seed2 {
                    continue;
                }

                // Добавляем в группу с минимальным увеличением площади
                let mut bbox1 = BoundingBox::from_point(group1[0].point);
                for e in &group1 {
                    bbox1.expand(&BoundingBox::from_point(e.point));
                }

                let mut bbox2 = BoundingBox::from_point(group2[0].point);
                for e in &group2 {
                    bbox2.expand(&BoundingBox::from_point(e.point));
                }

                let mut expanded1 = bbox1;
                expanded1.expand(&BoundingBox::from_point(entry.point));
                let mut expanded2 = bbox2;
                expanded2.expand(&BoundingBox::from_point(entry.point));

                let enlargement1 = expanded1.area() - bbox1.area();
                let enlargement2 = expanded2.area() - bbox2.area();

                if enlargement1 < enlargement2 {
                    group1.push(entry.clone());
                } else {
                    group2.push(entry.clone());
                }
            }

            *entries = group1;
            node.update_bbox();

            let mut new_node = RTreeNode::new_leaf();
            if let RTreeNode::Leaf {
                entries: ref mut e, ..
            } = new_node
            {
                *e = group2;
            }
            new_node.update_bbox();

            Box::new(new_node)
        } else {
            unreachable!()
        }
    }

    /// Split внутреннего узла.
    #[allow(dead_code)]
    fn split_internal(node: &mut RTreeNode) -> Box<RTreeNode> {
        if let RTreeNode::Internal { children, .. } = node {
            let (seed1, seed2) = Self::pick_seeds_internal(children);

            let mut group1 = vec![children[seed1].clone()];
            let mut group2 = vec![children[seed2].clone()];

            for (i, child) in children.iter().enumerate() {
                if i == seed1 || i == seed2 {
                    continue;
                }

                let mut bbox1 = *group1[0].bbox();
                for c in &group1 {
                    bbox1.expand(c.bbox());
                }

                let mut bbox2 = *group2[0].bbox();
                for c in &group2 {
                    bbox2.expand(c.bbox());
                }

                let mut expanded1 = bbox1;
                expanded1.expand(child.bbox());
                let mut expanded2 = bbox2;
                expanded2.expand(child.bbox());

                let enlargement1 = expanded1.area() - bbox1.area();
                let enlargement2 = expanded2.area() - bbox2.area();

                if enlargement1 < enlargement2 {
                    group1.push(child.clone());
                } else {
                    group2.push(child.clone());
                }
            }

            *children = group1;
            node.update_bbox();

            let mut new_node = RTreeNode::new_internal();
            if let RTreeNode::Internal {
                children: ref mut c,
                ..
            } = new_node
            {
                *c = group2;
            }
            new_node.update_bbox();

            Box::new(new_node)
        } else {
            unreachable!()
        }
    }

    /// Выбирает две самые далёкие записи для split.
    fn pick_seeds_leaf(entries: &[GeoEntry]) -> (usize, usize) {
        let mut max_dist = 0.0;
        let mut seed1 = 0;
        let mut seed2 = 1;

        for i in 0..entries.len() {
            for j in i + 1..entries.len() {
                let dist = Self::point_distance(entries[i].point, entries[j].point);
                if dist > max_dist {
                    max_dist = dist;
                    seed1 = i;
                    seed2 = j;
                }
            }
        }

        (seed1, seed2)
    }

    /// Выбирает два самых далёких дочерних узла для split.
    fn pick_seeds_internal(children: &[Box<RTreeNode>]) -> (usize, usize) {
        let mut max_dist = 0.0;
        let mut seed1 = 0;
        let mut seed2 = 1;

        for i in 0..children.len() {
            for j in i + 1..children.len() {
                let c1 = children[i].bbox().center();
                let c2 = children[j].bbox().center();
                let dist = Self::point_distance(c1, c2);
                if dist > max_dist {
                    max_dist = dist;
                    seed1 = i;
                    seed2 = j;
                }
            }
        }

        (seed1, seed2)
    }

    /// Простое евклидово расстояние (в градусах) для выбора seeds.
    fn point_distance(
        p1: GeoPoint,
        p2: GeoPoint,
    ) -> f64 {
        let dx = p2.lon - p1.lon;
        let dy = p2.lat - p1.lat;
        (dx * dx + dy * dy).sqrt()
    }

    /// Range query: все точки в bounding box.
    pub fn range_query(
        &self,
        bbox: &BoundingBox,
    ) -> Vec<GeoEntry> {
        let mut results = Vec::new();
        if let Some(ref root) = self.root {
            Self::range_query_recursive(root, bbox, &mut results);
        }
        results
    }

    fn range_query_recursive(
        node: &RTreeNode,
        query_bbox: &BoundingBox,
        results: &mut Vec<GeoEntry>,
    ) {
        if !node.bbox().intersects(query_bbox) {
            return;
        }

        match node {
            RTreeNode::Leaf { entries, .. } => {
                for entry in entries {
                    if query_bbox.contains_point(entry.point) {
                        results.push(entry.clone());
                    }
                }
            }
            RTreeNode::Internal { children, .. } => {
                for child in children {
                    Self::range_query_recursive(child, query_bbox, results);
                }
            }
        }
    }

    /// k-NN search: k ближайших соседей к точке.
    /// k-NN search: k ближайших соседей к точке.
    pub fn knn(
        &self,
        point: GeoPoint,
        k: usize,
    ) -> Vec<(GeoEntry, f64)> {
        if self.root.is_none() || k == 0 {
            return Vec::new();
        }

        // Priority queue для branch-and-bound search
        let mut pq: BinaryHeap<PqItem> = BinaryHeap::new();
        pq.push(PqItem {
            dist: 0.0,
            node: self.root.as_ref().unwrap().as_ref(),
            entry: None,
        });

        let mut results: BinaryHeap<ResultItem> = BinaryHeap::new();

        while let Some(item) = pq.pop() {
            // Pruning: если текущая дистанция больше k-го результата, пропускаем
            if results.len() >= k {
                // peek даёт самый далёкий в настоящий момент (max-heap)
                if item.dist > results.peek().unwrap().dist {
                    continue;
                }
            }

            if let Some(entry) = item.entry {
                // Leaf entry
                results.push(ResultItem {
                    entry: entry.clone(),
                    dist: item.dist,
                });
                if results.len() > k {
                    results.pop(); // Удаляем самый дальний
                }
            } else {
                // Internal node - добавляем дочерние узлы или записи
                match item.node {
                    RTreeNode::Leaf { entries, .. } => {
                        for entry in entries {
                            let dist = haversine_distance(point, entry.point);
                            pq.push(PqItem {
                                dist,
                                node: item.node,
                                entry: Some(entry),
                            });
                        }
                    }
                    RTreeNode::Internal { children, .. } => {
                        for child in children {
                            let dist = child.bbox().min_distance_to_point(point);
                            pq.push(PqItem {
                                dist,
                                node: child.as_ref(),
                                entry: None,
                            });
                        }
                    }
                }
            }
        }

        // Переведём результаты в Vec и отсортируем по возрастанию дистанции.
        // При равных дистанциях используем member как tie-breaker для
        // детерминированности.
        let mut vec: Vec<(GeoEntry, f64)> = results
            .into_vec()
            .into_iter()
            .map(|r| (r.entry, r.dist))
            .collect();

        vec.sort_by(
            |a, b| match a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal) {
                Ordering::Equal => a.0.member.cmp(&b.0.member),
                ord => ord,
            },
        );

        vec
    }

    /// Возвращает количество точек в дереве.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Проверяет, пусто ли дерево.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Статистика дерева (глубина, количество узлов).
    pub fn stats(&self) -> TreeStats {
        if let Some(ref root) = self.root {
            Self::compute_stats(root.as_ref(), 0)
        } else {
            TreeStats {
                depth: 0,
                node_count: 0,
                leaf_count: 0,
            }
        }
    }

    fn compute_stats(
        node: &RTreeNode,
        level: usize,
    ) -> TreeStats {
        match node {
            RTreeNode::Leaf { .. } => TreeStats {
                depth: level + 1,
                node_count: 1,
                leaf_count: 1,
            },
            RTreeNode::Internal { children, .. } => {
                let mut stats = TreeStats {
                    depth: level + 1,
                    node_count: 1,
                    leaf_count: 0,
                };
                for child in children {
                    let child_stats = Self::compute_stats(child, level + 1);
                    stats.depth = stats.depth.max(child_stats.depth);
                    stats.node_count += child_stats.node_count;
                    stats.leaf_count += child_stats.leaf_count;
                }
                stats
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для RTree, PqItem, ResultItem
////////////////////////////////////////////////////////////////////////////////

impl Default for RTree {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> PartialEq for PqItem<'a> {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.dist == other.dist
    }
}

impl<'a> Eq for PqItem<'a> {}

impl<'a> PartialOrd for PqItem<'a> {
    fn partial_cmp(
        &self,
        other: &Self,
    ) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for PqItem<'a> {
    fn cmp(
        &self,
        other: &Self,
    ) -> Ordering {
        // Min-heap: меньшая дистанция = выше приоритет
        other
            .dist
            .partial_cmp(&self.dist)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialEq for ResultItem {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.dist == other.dist
    }
}

impl Eq for ResultItem {}

impl PartialOrd for ResultItem {
    fn partial_cmp(
        &self,
        other: &Self,
    ) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ResultItem {
    fn cmp(
        &self,
        other: &Self,
    ) -> Ordering {
        // Max-heap: большая дистанция вверху
        self.dist
            .partial_cmp(&other.dist)
            .unwrap_or(Ordering::Equal)
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        member: &str,
        lon: f64,
        lat: f64,
    ) -> GeoEntry {
        GeoEntry {
            member: member.to_string(),
            point: GeoPoint { lon, lat },
            score: 0,
        }
    }

    #[test]
    fn test_bbox_operations() {
        let mut bbox = BoundingBox::from_point(GeoPoint { lon: 0.0, lat: 0.0 });
        assert_eq!(bbox.area(), 0.0);

        bbox.expand(&BoundingBox::new(1.0, 2.0, 1.0, 2.0));
        assert_eq!(bbox.min_lon, 0.0);
        assert_eq!(bbox.max_lon, 2.0);
        assert_eq!(bbox.area(), 4.0);
    }

    #[test]
    fn test_insert_and_range_query() {
        let mut tree = RTree::new();
        tree.insert(make_entry("A", 0.0, 0.0));
        tree.insert(make_entry("B", 1.0, 1.0));
        tree.insert(make_entry("C", 2.0, 2.0));

        let results = tree.range_query(&BoundingBox::new(-0.5, 1.5, -0.5, 1.5));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_bulk_load() {
        let entries = vec![
            make_entry("A", 0.0, 0.0),
            make_entry("B", 1.0, 1.0),
            make_entry("C", 2.0, 2.0),
            make_entry("D", 3.0, 3.0),
        ];
        let tree = RTree::bulk_load(entries);
        assert_eq!(tree.len(), 4);

        let stats = tree.stats();
        assert!(stats.depth <= 3);
    }

    #[test]
    fn test_knn() {
        let mut tree = RTree::new();
        tree.insert(make_entry("A", 0.0, 0.0));
        tree.insert(make_entry("B", 1.0, 0.0));
        tree.insert(make_entry("C", 2.0, 0.0));
        tree.insert(make_entry("D", 3.0, 0.0));

        let results = tree.knn(GeoPoint { lon: 0.5, lat: 0.0 }, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.member, "A");
        assert_eq!(results[1].0.member, "B");
    }

    #[test]
    fn test_large_dataset() {
        let mut tree = RTree::new();
        for i in 0..1000 {
            let lon = (i % 100) as f64 * 0.1;
            let lat = (i / 100) as f64 * 0.1;
            tree.insert(make_entry(&format!("P{}", i), lon, lat));
        }

        assert_eq!(tree.len(), 1000);
        let stats = tree.stats();
        assert!(stats.depth < 10);
    }
}
