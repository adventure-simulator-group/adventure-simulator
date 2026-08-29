#!/usr/bin/env python3
"""Publish every available authored MHR motion as an animation-only GLB.

Ordinary motions are validated against the runtime base and stripped of mesh
data. Walk and run are rebuilt as closed cycles. Canonical left-side motions
also produce their bind-relative mirrored runtime counterparts. Missing source
motions are skipped so the pack can be rebuilt incrementally while animating.
"""

from __future__ import annotations

import argparse
import copy
from contextlib import nullcontext
from dataclasses import dataclass
from pathlib import Path
import tempfile

import numpy as np

from build_locomotion_cycles import MOTIONS as LOCOMOTION_MOTIONS, build_cycle
from mirror_gait_assets import MIRRORED_MOTIONS, mirrored_glb
from prepare_animation_motion import (
    ANIMATION_FPS,
    accessor_view,
    append_float_accessor,
    prepare_motion,
)
from prepare_rig_base import GlbError, encode_glb, read_glb


ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIR = ROOT / "assets_src" / "biped" / "unarmed"
RUNTIME_DIR = ROOT / "assets" / "animations" / "biped" / "unarmed"
RUNTIME_BASE = RUNTIME_DIR / "base.glb"
TWO_HANDED_CLOSE_SOURCE_DIR = ROOT / "assets_src" / "biped" / "2h_close"
TWO_HANDED_CLOSE_RUNTIME_DIR = ROOT / "assets" / "animations" / "biped" / "2h_close"
GRIP_SOURCE_DIR = ROOT / "assets_src" / "biped"
GRIP_RUNTIME_DIR = ROOT / "assets" / "animations" / "biped"
GRIP_POSES = ("grip_hilt", "grip_polearm")
BARE_KNUCKLE_OVERLAY = GRIP_SOURCE_DIR / "grip_bare_knuckle.glb"

# Keep this table aligned with AnimationPackCatalog::biped_root. Generated
# locomotion and mirrored counterparts are deliberately absent.
DIRECT_MOTIONS = {
    "idle_relaxed": (0,),
    "prone_idle": (0,),
    "supine_idle": (0,),
    "prone_crawl": (0,),
    "prone_strafe": tuple(range(7)),
    "supine_scamper": (0,),
    "dive": (0,),
    "airborne_center": (0,),
    "airborne_travel": (0,),
    "swing": (0, 4, 8, 12),
    "thrust": (0, 4, 8, 12),
    "offhand": (0, 4),
    "prone_transition": (0,),
    "prone_supine_roll_left": (0,),
    "supine_transition": (0,),
    "combat_stance": (0,),
    # Preserve Cascadeur's baked in-betweens: reducing these motions to only
    # their five authored landmarks measurably changes the feet between keys.
    # Lateral root translation is still neutralized below.
    "quickstep_forward": tuple(range(13)),
    "quickstep_right": tuple(range(13)),
    "quickstep_left": tuple(range(13)),
    "quickstep_back": tuple(range(13)),
}

COMBAT_CYCLE_MOTIONS = ("strafe", "skip")
COMBAT_CYCLE_AUTHORED_FRAMES = (0, 6, 12, 18)
COMBAT_CYCLE_LAST_FRAME = 24

VARIABLE_ATTACK_FRAMES = {"swing", "thrust", "offhand"}


def available_authored_frames(
    source: Path, candidates: tuple[int, ...]
) -> tuple[int, ...]:
    """Return canonical attack anchors covered by the authored GLB duration."""
    document, binary = read_glb(source)
    try:
        animation = document["animations"][0]
        duration = max(
            float(accessor_view(document, binary, sampler["input"])[-1, 0])
            for sampler in animation["samplers"]
        )
    except (KeyError, IndexError, TypeError, ValueError) as error:
        raise GlbError(f"animation duration is malformed: {source}") from error
    return tuple(
        frame
        for frame in candidates
        if frame / ANIMATION_FPS <= duration + 0.5 / ANIMATION_FPS
    )


@dataclass(frozen=True)
class PublicationReport:
    published: tuple[str, ...]
    skipped: tuple[str, ...]


def publish_attack_pack(
    source_dir: Path,
    runtime_dir: Path,
    runtime_base: Path,
    *,
    check: bool = False,
) -> PublicationReport:
    """Publish one specialized pack containing only authored attack motions."""
    unknown = sorted(
        path.stem
        for path in source_dir.glob("*.glb")
        if path.stem not in VARIABLE_ATTACK_FRAMES
    )
    if unknown:
        raise GlbError(
            "specialized attack motions are absent from the publication contract: "
            + ", ".join(unknown)
        )

    published: list[str] = []
    skipped: list[str] = []
    for motion in sorted(VARIABLE_ATTACK_FRAMES):
        source = source_dir / f"{motion}.glb"
        if not source.is_file():
            skipped.append(motion)
            continue
        kept_frames = available_authored_frames(source, DIRECT_MOTIONS[motion])
        required_frames = 1 if motion == "offhand" else 2
        if len(kept_frames) < required_frames:
            raise GlbError(f"{motion} does not expose its required attack anchors")
        prepare_motion(
            source,
            runtime_base,
            runtime_dir / f"{motion}.glb",
            last_frame=max(kept_frames),
            kept_frames=kept_frames,
            check=check,
        )
        published.append(motion)
    return PublicationReport(tuple(published), tuple(skipped))


def close_combat_cycle(source: Path) -> bytes:
    """Copy every frame-0 key to frame 24, closing a 0/6/12/18 combat cycle."""
    document, source_binary = read_glb(source)
    closed = copy.deepcopy(document)
    binary = bytearray(source_binary)
    try:
        animation = closed["animations"][0]
        source_animation = document["animations"][0]
    except (KeyError, IndexError, TypeError) as error:
        raise GlbError(f"combat cycle animation is malformed: {source}") from error
    if len(closed.get("animations", ())) != 1:
        raise GlbError(f"combat cycle must contain exactly one animation: {source}")

    for sampler, source_sampler in zip(
        animation.get("samplers", ()), source_animation.get("samplers", ())
    ):
        interpolation = source_sampler.get("interpolation", "LINEAR")
        if interpolation not in {"LINEAR", "STEP"}:
            raise GlbError("combat cycle source must use LINEAR or STEP interpolation")
        times = accessor_view(document, source_binary, source_sampler["input"])[:, 0]
        values = accessor_view(document, source_binary, source_sampler["output"])
        zero = np.flatnonzero(np.isclose(times, 0.0, atol=1e-5))
        if zero.size != 1:
            raise GlbError("combat cycle must expose exactly one frame-0 key per track")
        required = COMBAT_CYCLE_AUTHORED_FRAMES[-1] / ANIMATION_FPS
        if float(times[-1]) + 0.5 / ANIMATION_FPS < required:
            raise GlbError("combat cycle does not cover its authored frame-18 pose")
        cycle_time = COMBAT_CYCLE_LAST_FRAME / ANIMATION_FPS
        before_close = times < cycle_time - 1e-5
        closed_times = np.concatenate(
            (
                times[before_close],
                np.asarray([cycle_time], dtype="<f4"),
            )
        )
        closed_values = np.concatenate(
            (values[before_close], values[zero[0] : zero[0] + 1]), axis=0
        )
        sampler["input"] = append_float_accessor(
            closed,
            binary,
            closed_times.reshape(-1, 1),
            "SCALAR",
            minimum=[float(closed_times[0])],
            maximum=[float(closed_times[-1])],
        )
        output_type = document["accessors"][source_sampler["output"]]["type"]
        sampler["output"] = append_float_accessor(
            closed, binary, closed_values, output_type
        )
    closed["buffers"][0]["byteLength"] = len(binary)
    return encode_glb(closed, bytes(binary))


def write_generated(path: Path, payload: bytes, *, check: bool) -> None:
    if check:
        if not path.is_file() or path.read_bytes() != payload:
            raise GlbError(f"runtime animation is stale: {path}")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.is_file() or path.read_bytes() != payload:
        path.write_bytes(payload)


def publish_animation_assets(
    *,
    source_dir: Path = SOURCE_DIR,
    runtime_dir: Path = RUNTIME_DIR,
    runtime_base: Path = RUNTIME_BASE,
    grip_source_dir: Path = GRIP_SOURCE_DIR,
    grip_runtime_dir: Path = GRIP_RUNTIME_DIR,
    bare_knuckle_overlay: Path = BARE_KNUCKLE_OVERLAY,
    check: bool = False,
) -> PublicationReport:
    if not runtime_base.is_file():
        raise GlbError(f"runtime MHR base is unavailable: {runtime_base}")

    generated_names = (
        set(LOCOMOTION_MOTIONS)
        | set(COMBAT_CYCLE_MOTIONS)
        | set(MIRRORED_MOTIONS.values())
    )
    # Retain superseded authoring sources without publishing them. Artists may
    # keep the old files while the runtime contract now derives guard from the
    # frame-0 attack anchors and continuation from frames 8/12.
    allowed_sources = {
        "base",
        "guard",
        "swing_follow",
        *DIRECT_MOTIONS,
        *LOCOMOTION_MOTIONS,
    }
    unknown = sorted(
        path.stem
        for path in source_dir.glob("*.glb")
        if path.stem not in allowed_sources and path.stem not in generated_names
    )
    if unknown:
        raise GlbError(
            "source motions are absent from the publication contract: "
            + ", ".join(unknown)
        )
    available_attacks = sorted(
        motion
        for motion in VARIABLE_ATTACK_FRAMES
        if (source_dir / f"{motion}.glb").is_file()
    )
    if available_attacks and not bare_knuckle_overlay.is_file():
        raise GlbError(
            "bare-knuckle grip overlay is required to publish unarmed attacks: "
            f"{bare_knuckle_overlay}"
        )

    published: list[str] = []
    skipped: list[str] = []
    for motion in GRIP_POSES:
        source = grip_source_dir / f"{motion}.glb"
        if not source.is_file():
            skipped.append(motion)
            continue
        prepare_motion(
            source,
            runtime_base,
            grip_runtime_dir / f"{motion}.glb",
            last_frame=0,
            kept_frames=(0,),
            target_subtree_roots=("l_wrist", "r_wrist"),
            preserve_default_target_nodes=("r_weapon",) if motion == "grip_hilt" else (),
            check=check,
        )
        published.append(motion)

    for motion, kept_frames in DIRECT_MOTIONS.items():
        source = source_dir / f"{motion}.glb"
        if not source.is_file():
            skipped.append(motion)
            continue
        if motion in VARIABLE_ATTACK_FRAMES:
            kept_frames = available_authored_frames(source, kept_frames)
            required_frames = 1 if motion == "offhand" else 2
            if len(kept_frames) < required_frames:
                raise GlbError(
                    f"{motion} does not expose its required attack anchors"
                )
        uses_bare_knuckles = motion in VARIABLE_ATTACK_FRAMES
        temporary_overlay = (
            tempfile.TemporaryDirectory() if uses_bare_knuckles else nullcontext(None)
        )
        with temporary_overlay as temporary:
            overlay_poses: tuple[tuple[Path, tuple[str, ...]], ...] = ()
            if uses_bare_knuckles:
                mirrored_overlay = Path(temporary) / "grip_bare_knuckle_mirrored.glb"
                mirrored_overlay.write_bytes(mirrored_glb(bare_knuckle_overlay))
                overlay_poses = (
                    (bare_knuckle_overlay, ("r_wrist",)),
                    (mirrored_overlay, ("l_wrist",)),
                )
            prepare_motion(
                source,
                runtime_base,
                runtime_dir / f"{motion}.glb",
                last_frame=max(kept_frames),
                kept_frames=kept_frames,
                remove_root_lateral_motion=motion.startswith("quickstep_"),
                overlay_poses=overlay_poses,
                overlay_target_subtree_roots=("l_wrist", "r_wrist"),
                check=check,
            )
        published.append(motion)

    for motion in LOCOMOTION_MOTIONS:
        source = source_dir / f"{motion}.glb"
        if not source.is_file():
            skipped.append(motion)
            continue
        write_generated(
            runtime_dir / f"{motion}.glb",
            build_cycle(source),
            check=check,
        )
        published.append(motion)
        mirrored_motion = MIRRORED_MOTIONS.get(motion)
        if mirrored_motion is not None:
            write_generated(
                runtime_dir / f"{mirrored_motion}.glb",
                build_cycle(source, mirrored_start=True),
                check=check,
            )
            published.append(mirrored_motion)

    for motion in COMBAT_CYCLE_MOTIONS:
        source = source_dir / f"{motion}.glb"
        if not source.is_file():
            skipped.append(motion)
            continue
        with tempfile.TemporaryDirectory() as temporary:
            closed_source = Path(temporary) / f"{motion}.glb"
            closed_source.write_bytes(close_combat_cycle(source))
            prepare_motion(
                closed_source,
                runtime_base,
                runtime_dir / f"{motion}.glb",
                last_frame=COMBAT_CYCLE_LAST_FRAME,
                kept_frames=(*COMBAT_CYCLE_AUTHORED_FRAMES, COMBAT_CYCLE_LAST_FRAME),
                check=check,
            )
        published.append(motion)

    for source_motion, output_motion in MIRRORED_MOTIONS.items():
        if source_motion in LOCOMOTION_MOTIONS:
            continue
        source = source_dir / f"{source_motion}.glb"
        if source_motion not in published:
            skipped.append(output_motion)
            continue
        kept_frames = DIRECT_MOTIONS[source_motion]
        with tempfile.TemporaryDirectory() as temporary:
            mirrored_source = Path(temporary) / f"{output_motion}.glb"
            mirrored_source.write_bytes(mirrored_glb(source))
            prepare_motion(
                mirrored_source,
                runtime_base,
                runtime_dir / f"{output_motion}.glb",
                last_frame=max(kept_frames),
                kept_frames=kept_frames,
                remove_root_lateral_motion=output_motion.startswith("quickstep_"),
                check=check,
            )
        published.append(output_motion)

    return PublicationReport(tuple(published), tuple(skipped))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        report = publish_animation_assets(check=args.check)
        two_handed_report = publish_attack_pack(
            TWO_HANDED_CLOSE_SOURCE_DIR,
            TWO_HANDED_CLOSE_RUNTIME_DIR,
            RUNTIME_BASE,
            check=args.check,
        )
    except (GlbError, OSError, ValueError) as error:
        parser.error(str(error))
    action = "verified" if args.check else "published"
    count = len(report.published) + len(two_handed_report.published)
    print(f"{action} {count} animation GLBs")
    skipped = [*report.skipped, *(f"2h_close/{name}" for name in two_handed_report.skipped)]
    if skipped:
        print("skipped missing sources: " + ", ".join(skipped))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
