"""Create one-point and two-point off-hand constraint variants of swing.casc.

Run it once per variant through Cascadeur's CLI, setting
``CASCADEUR_CONSTRAINT_VARIANT`` to ``one`` or ``two``.  Each invocation clones
the source before loading its output, so the source is never saved or modified.
"""

from pathlib import Path
import os
import shutil
import traceback

import csc
import pycsc
import commands.constrain.add_constraint as add_constraint
import commands.constrain.delete_point_constraints as delete_point_constraints


SOURCE = Path(r"C:\Users\adler\projects\fabelgeist\assets_src\biped\2h_close\swing.casc")
OUTPUTS = (
    (
        "one",
        SOURCE.with_name("swing_left_hand_one_point_constraint.casc"),
        ("wrist_MainPoint_l",),
    ),
    (
        "two",
        SOURCE.with_name("swing_left_hand_two_point_constraint.casc"),
        ("wrist_MainPoint_l", "wrist_DirectionPoint_l"),
    ),
)
WRIST_POINTS = (
    "wrist_MainPoint_l",
    "wrist_DirectionPoint_l",
    "wrist_AdditionalPoint_l",
)
DRIVER_NAME = "weapon_Triangle_r"
REPORT = SOURCE.with_name("swing_constraint_variants_report.txt")


def _one_object(scene, name):
    objects = list(scene.model_viewer().get_objects(name=name))
    exact = [obj_id for obj_id in objects if scene.model_viewer().get_object_name(obj_id) == name]
    if len(exact) != 1:
        matches = [scene.model_viewer().get_object_name(obj_id) for obj_id in objects]
        raise RuntimeError(f"Expected exactly one object named {name!r}; found {matches!r}")
    return exact[0]


def _activate_across_scene(scene, point_ids):
    frame_count = scene.layers_viewer().frames_count()

    def activate(_model, update, _scene_updater):
        for point_id in point_ids:
            constraint_index = (
                update.get_object_by_id(point_id)
                .root_group()
                .node_deep("Constraint Index")
            )
            for frame in range(frame_count):
                constraint_index.set_value(1, frame)

    if not scene.modify_update("Activate left-hand weapon constraint", activate):
        raise RuntimeError("Cascadeur rejected Constraint Index activation")


def _safe_remove_constraint_from_relaxation(
    point, constraint_node, constraint_name, py_scene, _model, _scene
):
    """Cascadeur's stock remover lacks a null check needed by this source scene."""
    point_root = point.node.root_group()
    wrapped_constraint = pycsc.wrap(constraint_node, py_scene)
    if not wrapped_constraint:
        return
    local_rigid_position = wrapped_constraint.node_deep("Local Rigidbody Position")
    if not local_rigid_position:
        return
    local_rigid_position_id = local_rigid_position.data_id()

    for connection in point.get_behaviours_by_name("ConnectionPointTwoBody"):
        if connection is None or connection.is_null():
            continue
        local_position_property = connection.pos_local_first
        if local_position_property is None:
            continue
        connection_id = local_position_property.id
        if connection_id is None or connection_id.is_null():
            continue
        if connection_id == local_rigid_position_id:
            connection.delete_self()
            break

    for attribute_name in (
        f"{constraint_name} Rigid Position",
        f"{constraint_name} Rigid Rotation",
    ):
        if point_root.has_input_attr(attribute_name):
            point_root.remove_attribute(point_root.input_attr(attribute_name))


def _delete_existing_constraints(scene, point_ids):
    original = delete_point_constraints.remove_constraint_from_relaxation
    delete_point_constraints.remove_constraint_from_relaxation = (
        _safe_remove_constraint_from_relaxation
    )
    try:
        delete_point_constraints.delete_constraints(scene, point_ids)
    finally:
        delete_point_constraints.remove_constraint_from_relaxation = original

    # The source scene was saved after failed GUI constraint attempts.  In
    # 2026.1.3 the stock deletion command can remove the constraint node while
    # leaving its interface sockets behind; a subsequent bind then reconnects
    # duplicate Position/Rotation inputs and the transaction rolls back.
    stale_driver_names = ("weapon", "weapon_Triangle_r")

    def remove_stale_driver_inputs(model, update, scene_updater):
        py_scene = pycsc.wrap(scene)
        py_scene.set_modifiers(model, update, scene_updater)
        for point_id in point_ids:
            update_root = update.get_object_by_id(point_id).root_group()
            for node in list(update_root.nodes()):
                if " constraint" in node.name():
                    update.delete_node(node.id())

            root = pycsc.wrap(point_id, py_scene).node.root_group()
            for driver_name in stale_driver_names:
                for suffix in (
                    " Position",
                    " Rotation",
                    " Rigid Position",
                    " Rigid Rotation",
                ):
                    attribute_name = driver_name + suffix
                    if root.has_input_attr(attribute_name):
                        root.remove_attribute(root.input_attr(attribute_name))

    if not scene.modify("Remove stale weapon constraint inputs", remove_stale_driver_inputs):
        raise RuntimeError("Cascadeur rejected stale weapon constraint cleanup")


def _add_constraints(scene, point_ids, driver_id):
    original_create_clusters = add_constraint.create_clusters_for_points

    def create_clusters_if_needed(
        model, current_scene, constrained_object, constraint_data, prev_constraint_data=None
    ):
        cluster_viewer = current_scene.model_viewer().data_viewer().cluster_viewer()
        if cluster_viewer.cluster_by_data(constraint_data) != -1:
            return
        original_create_clusters(
            model,
            current_scene,
            constrained_object,
            constraint_data,
            prev_constraint_data,
        )

    def add(model, update, scene_updater):
        driver = update.get_object_by_id(driver_id)
        prepared = add_constraint.prepare_selected_points(
            model, update, scene_updater, list(point_ids)
        )
        if set(prepared) != set(point_ids):
            raise RuntimeError(
                f"Cascadeur filtered requested points: requested={point_ids!r}, prepared={prepared!r}"
            )
        for point_id in prepared:
            add_constraint.constrain_single_point(
                model, update, scene_updater, driver, point_id, False
            )

    add_constraint.create_clusters_for_points = create_clusters_if_needed
    try:
        if not scene.modify("Create left-hand weapon constraint", add):
            raise RuntimeError("Cascadeur rejected creation of the weapon constraint")
    finally:
        add_constraint.create_clusters_for_points = original_create_clusters


def _verify(scene, constrained_names):
    results = []
    frame_count = scene.layers_viewer().frames_count()

    def inspect(_model, update, _scene_updater):
        mv = scene.model_viewer()
        dv = mv.data_viewer()
        for name in WRIST_POINTS:
            point_id = _one_object(scene, name)
            root = update.get_object_by_id(point_id).root_group()
            index_node = root.node_deep("Constraint Index")
            active_values = (
                []
                if index_node is None
                else [
                    dv.get_setting_value(index_node.data_id(), frame)
                    for frame in range(frame_count)
                ]
            )
            expected = name in constrained_names
            present = any(
                node.name() == f"{DRIVER_NAME} constraint" for node in root.nodes()
            )
            if present != expected:
                raise RuntimeError(
                    f"{name}: expected weapon constraint present={expected}, got {present}"
                )
            expected_index = 1 if expected else 0
            wrong_frames = [
                frame
                for frame, value in enumerate(active_values)
                if int(value) != expected_index
            ]
            if wrong_frames:
                raise RuntimeError(
                    f"{name}: expected Constraint Index {expected_index} on every frame; "
                    f"mismatches at {wrong_frames[:10]!r}"
                )
            results.append(
                f"{name}: present={present}, frames=0-{frame_count - 1}, "
                f"index={expected_index}"
            )

    if not scene.modify("Verify left-hand weapon constraint", inspect):
        raise RuntimeError("Cascadeur rejected constraint verification")
    return results


def _create_variant(app, output_path, constrained_names):
    shutil.copy2(SOURCE, output_path)

    scene_manager = app.get_scene_manager()
    data_sources = app.get_data_source_manager()
    application_scene = scene_manager.create_application_scene()
    try:
        scene_manager.set_current_scene(application_scene)
        if not data_sources.load_scene(str(output_path)):
            raise RuntimeError(f"Failed to load cloned scene: {output_path}")

        scene = application_scene.domain_scene()
        mv = scene.model_viewer()
        driver_id = _one_object(scene, DRIVER_NAME)
        driver_type = mv.get_object_type_name(driver_id)
        all_wrist_ids = [_one_object(scene, name) for name in WRIST_POINTS]
        constrained_ids = [_one_object(scene, name) for name in constrained_names]

        # The source has remnants from earlier constraint attempts. Remove every
        # point constraint on the wrist before constructing the demonstration.
        _delete_existing_constraints(scene, all_wrist_ids)
        _add_constraints(scene, constrained_ids, driver_id)
        _activate_across_scene(scene, constrained_ids)
        verification = _verify(scene, set(constrained_names))

        application_scene.save(str(output_path))
        return [
            f"output={output_path}",
            f"driver={DRIVER_NAME} ({driver_type})",
            f"constrained={', '.join(constrained_names)}",
            *verification,
        ]
    finally:
        data_sources.close_scene(application_scene)


def run(_scene):
    app = csc.app.get_application()
    requested_variant = os.environ.get("CASCADEUR_CONSTRAINT_VARIANT", "").lower()
    selected_outputs = [item for item in OUTPUTS if item[0] == requested_variant]
    report_lines = [f"source={SOURCE}", f"variant={requested_variant}"]
    try:
        if not SOURCE.is_file():
            raise FileNotFoundError(SOURCE)
        if not selected_outputs:
            raise ValueError(
                "CASCADEUR_CONSTRAINT_VARIANT must be 'one' or 'two'"
            )
        for variant, output_path, constrained_names in selected_outputs:
            report_lines.append("")
            report_lines.extend(_create_variant(app, output_path, constrained_names))
            variant_report = REPORT.with_name(
                f"swing_{variant}_point_constraint_report.txt"
            )
            variant_report.write_text(
                "\n".join(report_lines + ["", "status=success"]) + "\n",
                encoding="utf-8",
            )
        report_lines.append("")
        report_lines.append("status=success")
        REPORT.write_text("\n".join(report_lines) + "\n", encoding="utf-8")
    except Exception:
        report_lines.append("")
        report_lines.append("status=failed")
        report_lines.append(traceback.format_exc())
        REPORT.write_text("\n".join(report_lines) + "\n", encoding="utf-8")
        raise
