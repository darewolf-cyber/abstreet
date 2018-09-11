// Copyright 2018 Google LLC, licensed under http://www.apache.org/licenses/LICENSE-2.0

use aabb_quadtree::geom::{Point, Rect};
use aabb_quadtree::QuadTree;
use control::ControlMap;
use geom::{LonLat, Pt2D};
use kml::{ExtraShape, ExtraShapeID};
use map_model::{BuildingID, IntersectionID, Lane, LaneID, Map, ParcelID, Turn, TurnID};
use plugins::selection::Hider;
use render::building::DrawBuilding;
use render::extra_shape::DrawExtraShape;
use render::intersection::DrawIntersection;
use render::lane::DrawLane;
use render::parcel::DrawParcel;
use render::turn::DrawTurn;
use render::Renderable;
use std::collections::HashMap;

pub struct DrawMap {
    pub lanes: Vec<DrawLane>,
    pub intersections: Vec<DrawIntersection>,
    pub turns: HashMap<TurnID, DrawTurn>,
    pub buildings: Vec<DrawBuilding>,
    pub parcels: Vec<DrawParcel>,
    pub extra_shapes: Vec<DrawExtraShape>,

    lanes_quadtree: QuadTree<LaneID>,
    intersections_quadtree: QuadTree<IntersectionID>,
    buildings_quadtree: QuadTree<BuildingID>,
    parcels_quadtree: QuadTree<ParcelID>,
    extra_shapes_quadtree: QuadTree<ExtraShapeID>,
}

impl DrawMap {
    // Also returns the center of the map in map-space
    pub fn new(
        map: &Map,
        control_map: &ControlMap,
        raw_extra_shapes: Vec<ExtraShape>,
    ) -> (DrawMap, Pt2D) {
        let mut lanes: Vec<DrawLane> = Vec::new();
        for l in map.all_lanes() {
            lanes.push(DrawLane::new(l, map, control_map));
        }

        let mut turn_to_lane_offset: HashMap<TurnID, usize> = HashMap::new();
        for l in map.all_lanes() {
            DrawMap::compute_turn_to_lane_offset(&mut turn_to_lane_offset, l, map);
        }
        assert_eq!(turn_to_lane_offset.len(), map.all_turns().len());

        let mut turns: HashMap<TurnID, DrawTurn> = HashMap::new();
        for t in map.all_turns().values() {
            turns.insert(t.id, DrawTurn::new(map, t, turn_to_lane_offset[&t.id]));
        }
        let intersections: Vec<DrawIntersection> = map.all_intersections()
            .iter()
            .map(|i| DrawIntersection::new(i, map, &lanes))
            .collect();
        let buildings: Vec<DrawBuilding> = map.all_buildings()
            .iter()
            .map(|b| DrawBuilding::new(b))
            .collect();
        let parcels: Vec<DrawParcel> = map.all_parcels()
            .iter()
            .map(|p| DrawParcel::new(p))
            .collect();

        let extra_shapes: Vec<DrawExtraShape> = raw_extra_shapes
            .into_iter()
            .map(|s| DrawExtraShape::new(s))
            .collect();

        // min_y here due to the wacky y inversion
        let bounds = map.get_gps_bounds();
        let max_screen_pt = Pt2D::from_gps(&LonLat::new(bounds.max_x, bounds.min_y), &bounds);
        let map_bbox = Rect {
            top_left: Point { x: 0.0, y: 0.0 },
            bottom_right: Point {
                x: max_screen_pt.x() as f32,
                y: max_screen_pt.y() as f32,
            },
        };

        let mut lanes_quadtree = QuadTree::default(map_bbox);
        for l in &lanes {
            lanes_quadtree.insert_with_box(l.id, l.get_bbox());
        }
        let mut intersections_quadtree = QuadTree::default(map_bbox);
        for i in &intersections {
            intersections_quadtree.insert_with_box(i.id, i.get_bbox());
        }
        let mut buildings_quadtree = QuadTree::default(map_bbox);
        for b in &buildings {
            buildings_quadtree.insert_with_box(b.id, b.get_bbox());
        }
        let mut parcels_quadtree = QuadTree::default(map_bbox);
        for p in &parcels {
            parcels_quadtree.insert_with_box(p.id, p.get_bbox());
        }

        let mut extra_shapes_quadtree = QuadTree::default(map_bbox);
        for s in &extra_shapes {
            extra_shapes_quadtree.insert_with_box(s.id, s.get_bbox());
        }

        (
            DrawMap {
                lanes,
                intersections,
                turns,
                buildings,
                parcels,
                extra_shapes,

                lanes_quadtree,
                intersections_quadtree,
                buildings_quadtree,
                parcels_quadtree,
                extra_shapes_quadtree,
            },
            Pt2D::new(max_screen_pt.x() / 2.0, max_screen_pt.y() / 2.0),
        )
    }

    fn compute_turn_to_lane_offset(result: &mut HashMap<TurnID, usize>, l: &Lane, map: &Map) {
        // Split into two groups, based on the endpoint
        let mut pair: (Vec<&Turn>, Vec<&Turn>) = map.get_turns_from_lane(l.id)
            .iter()
            .partition(|t| t.parent == l.dst_i);

        // Sort the turn icons by angle.
        pair.0
            .sort_by_key(|t| t.line.angle().normalized_degrees() as i64);
        pair.1
            .sort_by_key(|t| t.line.angle().normalized_degrees() as i64);

        for (idx, t) in pair.0.iter().enumerate() {
            result.insert(t.id, idx);
        }
        for (idx, t) in pair.1.iter().enumerate() {
            result.insert(t.id, idx);
        }
    }

    pub fn edit_lane_type(&mut self, id: LaneID, map: &Map, control_map: &ControlMap) {
        // No need to edit the quadtree; the bbox shouldn't depend on lane type.
        self.lanes[id.0] = DrawLane::new(map.get_l(id), map, control_map);
    }

    pub fn edit_remove_turn(&mut self, id: TurnID) {
        self.turns.remove(&id);
    }

    pub fn edit_add_turn(&mut self, id: TurnID, map: &Map) {
        let t = map.get_t(id);
        let mut turn_to_lane_offset: HashMap<TurnID, usize> = HashMap::new();
        DrawMap::compute_turn_to_lane_offset(&mut turn_to_lane_offset, map.get_l(t.src), map);
        let draw_turn = DrawTurn::new(map, t, turn_to_lane_offset[&id]);
        self.turns.insert(id, draw_turn);
    }

    // The alt to these is implementing std::ops::Index, but that's way more verbose!
    pub fn get_l(&self, id: LaneID) -> &DrawLane {
        &self.lanes[id.0]
    }

    pub fn get_i(&self, id: IntersectionID) -> &DrawIntersection {
        &self.intersections[id.0]
    }

    pub fn get_t(&self, id: TurnID) -> &DrawTurn {
        &self.turns[&id]
    }

    pub fn get_b(&self, id: BuildingID) -> &DrawBuilding {
        &self.buildings[id.0]
    }

    pub fn get_p(&self, id: ParcelID) -> &DrawParcel {
        &self.parcels[id.0]
    }

    pub fn get_es(&self, id: ExtraShapeID) -> &DrawExtraShape {
        &self.extra_shapes[id.0]
    }

    pub fn get_loads_onscreen(&self, screen_bbox: Rect, hider: &Hider) -> Vec<&DrawLane> {
        let mut v = Vec::new();
        for &(id, _, _) in &self.lanes_quadtree.query(screen_bbox) {
            if hider.show_l(*id) {
                v.push(self.get_l(*id));
            }
        }
        v
    }

    pub fn get_intersections_onscreen(
        &self,
        screen_bbox: Rect,
        hider: &Hider,
    ) -> Vec<&DrawIntersection> {
        let mut v = Vec::new();
        for &(id, _, _) in &self.intersections_quadtree.query(screen_bbox) {
            if hider.show_i(*id) {
                v.push(self.get_i(*id));
            }
        }
        v
    }

    pub fn get_buildings_onscreen(&self, screen_bbox: Rect, hider: &Hider) -> Vec<&DrawBuilding> {
        let mut v = Vec::new();
        for &(id, _, _) in &self.buildings_quadtree.query(screen_bbox) {
            if hider.show_b(*id) {
                v.push(self.get_b(*id));
            }
        }
        v
    }

    pub fn get_parcels_onscreen(&self, screen_bbox: Rect) -> Vec<&DrawParcel> {
        let mut v = Vec::new();
        for &(id, _, _) in &self.parcels_quadtree.query(screen_bbox) {
            v.push(self.get_p(*id));
        }
        v
    }

    pub fn get_extra_shapes_onscreen(
        &self,
        screen_bbox: Rect,
        hider: &Hider,
    ) -> Vec<&DrawExtraShape> {
        let mut v = Vec::new();
        for &(id, _, _) in &self.extra_shapes_quadtree.query(screen_bbox) {
            if hider.show_es(*id) {
                v.push(self.get_es(*id));
            }
        }
        v
    }
}
