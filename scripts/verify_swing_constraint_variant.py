"""Reopen one generated swing constraint variant and verify persisted state."""

import os
import traceback

import csc
from pycsc import data_constants as dc
import common.math_operations as math_operations

from scripts.create_swing_constraint_variants import (
    DRIVER_NAME,
    OUTPUTS,
    _one_object,
    _verify,
)


def _verify_driver_motion(scene, constrained_names):
    movement = 10.0 * dc.unit_x
    moved_names = []
    driver_point_names = (
        "weapon_MainPoint_r",
        "weapon_DirectionPoint_r",
        "weapon_AdditionalPoint_r",
    )

    def move_and_measure(_model, update, scene_updater):
        data_viewer = scene.model_viewer().data_viewer()
        point_positions = {}
        before = {}
        for name in constrained_names:
            point_id = _one_object(scene, name)
            position = (
                update.get_object_by_id(point_id).root_group().node_deep("Position")
            )
            point_positions[name] = position
            before[name] = data_viewer.get_data_value(position.data_id(), 0)

        driver_actuals = set()
        for name in driver_point_names:
            driver_point_id = _one_object(scene, name)
            driver_position = (
                update.get_object_by_id(driver_point_id)
                .root_group()
                .node_deep("Position")
            )
            driver_position.set_value(driver_position.value(0) + movement, 0)
            driver_actuals.add(driver_position.data_id())
        scene_updater.run_update(driver_actuals, 0)

        for name, position in point_positions.items():
            after = data_viewer.get_data_value(position.data_id(), 0)
            expected = before[name] + movement
            if not math_operations.compare_points(after, expected, 0.3):
                raise RuntimeError(
                    f"{name} did not follow a 10-unit driver translation: "
                    f"before={before[name]!r}, after={after!r}, expected={expected!r}"
                )
            moved_names.append(name)

    if not scene.modify_update("Verify constraint driver motion", move_and_measure):
        raise RuntimeError("Cascadeur rejected driver-motion verification")
    return f"motion_test=passed ({', '.join(moved_names)} followed {DRIVER_NAME})"


def run(_scene):
    requested = os.environ.get("CASCADEUR_CONSTRAINT_VARIANT", "").lower()
    matches = [item for item in OUTPUTS if item[0] == requested]
    if len(matches) != 1:
        raise ValueError("CASCADEUR_CONSTRAINT_VARIANT must be 'one' or 'two'")

    variant, output_path, constrained_names = matches[0]
    report_path = output_path.with_name(
        f"swing_{variant}_point_constraint_reload_report.txt"
    )
    report_lines = [f"output={output_path}", f"variant={variant}"]
    app = csc.app.get_application()
    scene_manager = app.get_scene_manager()
    data_sources = app.get_data_source_manager()
    application_scene = scene_manager.create_application_scene()
    try:
        scene_manager.set_current_scene(application_scene)
        if not data_sources.load_scene(str(output_path)):
            raise RuntimeError(f"Failed to load generated scene: {output_path}")
        verification = _verify(
            application_scene.domain_scene(), set(constrained_names)
        )
        report_lines.extend(verification)
        report_lines.append(
            _verify_driver_motion(application_scene.domain_scene(), constrained_names)
        )
        report_lines.append("status=success")
    except Exception:
        report_lines.extend(("status=failed", traceback.format_exc()))
        raise
    finally:
        report_path.write_text("\n".join(report_lines) + "\n", encoding="utf-8")
        data_sources.close_scene(application_scene)
