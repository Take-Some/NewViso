"""Unit contracts for the source-Ink -> engine.ui presentation compiler."""
import importlib.util
import json
import unittest
from pathlib import Path

SOURCE = Path(__file__).resolve().parents[1] / "build_ink_runtime_surfaces.py"
spec = importlib.util.spec_from_file_location("ink_surface_builder", SOURCE)
ink = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ink)


def node(kind, name="test", w=120, h=40, children=None, anchor="TopLeft"):
    return {
        "id": "test." + name, "component_id": "panel",
        "props": {
            "ink_class": kind, "ink_name": name, "ink_visible": 1,
            "ink_size": {"X": w, "Y": h},
            "ink_layout": {
                "anchor": anchor, "margin": {
                    "left":10,"top":20,"right":0,"bottom":0}},
            "ink_renderTransform": {"translation":{"X":0,"Y":0},
                                    "scale":{"X":1,"Y":1}},
        },
        "children": children or [],
    }


class InkSourceSurfaceCompilerTests(unittest.TestCase):
    def test_source_anchor_and_dimensions_are_stable(self):
        obj=node("inkTextWidget",w=1180,h=292)
        elems,counts,missing=ink.relative_source_nodes(obj)
        self.assertEqual(len(elems),1)
        rect=elems[0]["props"]
        self.assertAlmostEqual(rect["x_px"],10*1600/3840,places=3)
        self.assertAlmostEqual(rect["y_px"],20*900/2160,places=3)
        self.assertAlmostEqual(rect["w_px"],1180*1600/3840,places=3)

    def test_original_zero_size_image_is_not_stretched_fullscreen(self):
        obj=node("inkImageWidget",w=0,h=0)
        elems,counts,missing=ink.relative_source_nodes(obj)
        self.assertEqual(elems,[])
        self.assertEqual(counts["dynamic_leaf_geometry_deferred"],1)
        self.assertEqual(missing["runtime_controller_sized_widget"],1)

    def test_container_with_zero_size_retains_nested_children(self):
        c=node("inkCanvasWidget",w=0,h=0,
               children=[node("inkTextWidget",name="title",w=200,h=50)])
        elems,counts,missing=ink.relative_source_nodes(c)
        self.assertEqual(len(elems),1)
        self.assertEqual(elems[0]["id"],"test.title")

    def test_original_font_family_and_style_are_preserved(self):
        obj=node("inkTextWidget")
        obj["props"]["ink_fontFamily"]="base/gameplay/gui/fonts/raj/raj.inkfontfamily"
        obj["props"]["ink_fontStyle"]={"$value":"Medium"}
        elems,_,_=ink.relative_source_nodes(obj,{
            "base/gameplay/gui/fonts/raj/raj.inkfontfamily":"Raj"})
        self.assertEqual(elems[0]["props"]["font_ref"],
                         "font://cyberpunk/Raj/Medium")

    def test_missing_original_atlas_does_not_invent_asset(self):
        obj=node("inkImageWidget")
        obj["props"]["ink_textureAtlas"]="missing/source.inkatlas"
        obj["props"]["ink_texturePart"]={"$value":"source_sprite"}
        elems,_,_=ink.relative_source_nodes(obj)
        self.assertNotIn("texture_ref",elems[0]["props"])
        self.assertEqual(elems[0]["props"]["ink_part"],"source_sprite")


if __name__=="__main__":
    unittest.main()
