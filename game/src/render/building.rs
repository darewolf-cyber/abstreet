use crate::helpers::{ColorScheme, ID};
use crate::render::{DrawCtx, DrawOptions, Renderable, OUTLINE_THICKNESS};
use ezgui::{Color, GeomBatch, GfxCtx, Line, Text};
use geom::{Angle, Distance, Line, Polygon, Pt2D};
use map_model::{Building, BuildingID, Map, NORMAL_LANE_THICKNESS, SIDEWALK_THICKNESS};

pub struct DrawBuilding {
    pub id: BuildingID,
    label: Option<Text>,
    label_pos: Pt2D,
}

impl DrawBuilding {
    pub fn new(bldg: &Building, cs: &ColorScheme, batch: &mut GeomBatch) -> DrawBuilding {
        // Trim the front path line away from the sidewalk's center line, so that it doesn't
        // overlap. For now, this cleanup is visual; it doesn't belong in the map_model layer.
        let mut front_path_line = bldg.front_path.line.clone();
        let len = front_path_line.length();
        let trim_back = SIDEWALK_THICKNESS / 2.0;
        if len > trim_back && len - trim_back > geom::EPSILON_DIST {
            front_path_line = Line::new(
                front_path_line.pt1(),
                front_path_line.dist_along(len - trim_back),
            );
        }
        let front_path = front_path_line.make_polygons(Distance::meters(1.0));

        batch.push(
            cs.get_def("building", Color::rgb(196, 193, 188)),
            bldg.polygon.clone(),
        );
        batch.push(cs.get("sidewalk"), front_path);

        // TODO Do similar trim_back for driveway
        if let Some(ref p) = bldg.parking {
            batch.push(
                cs.get("driving lane"),
                p.driveway_line.make_polygons(NORMAL_LANE_THICKNESS),
            );
        }

        let label = bldg
            .osm_tags
            .get("addr:housenumber")
            .map(|num| Text::from(Line(num.to_string()).fg(Color::BLACK).size(50)));

        if bldg.parking.is_some() {
            // Might need to scale down more for some buildings, but so far, this works everywhere.
            batch.add_svg(
                "assets/map/parking.svg",
                bldg.label_center,
                0.1,
                Angle::ZERO,
            );
        }

        // TODO Slow and looks silly, but it's a nice experiment.
        /*for poly in bldg.polygon.shrink(-3.0) {
            batch.push(cs.get_def("building roof", Color::rgb(150, 75, 0)), poly);
        }*/

        DrawBuilding {
            id: bldg.id,
            label,
            label_pos: bldg.label_center,
        }
    }
}

impl Renderable for DrawBuilding {
    fn get_id(&self) -> ID {
        ID::Building(self.id)
    }

    fn draw(&self, g: &mut GfxCtx, opts: &DrawOptions, _: &DrawCtx) {
        if opts.label_buildings {
            if let Some(ref txt) = self.label {
                g.draw_text_at_mapspace(txt, self.label_pos);
            }
        }
    }

    // Some buildings cover up tunnels
    fn get_zorder(&self) -> isize {
        0
    }

    fn get_outline(&self, map: &Map) -> Polygon {
        map.get_b(self.id).polygon.to_outline(OUTLINE_THICKNESS)
    }

    fn contains_pt(&self, pt: Pt2D, map: &Map) -> bool {
        map.get_b(self.id).polygon.contains_pt(pt)
    }
}
