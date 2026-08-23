#!/usr/bin/env python3
"""Publish every available authored MHR motion as an animation-only GLB.

Ordinary motions are validated against the runtime base and stripped of mesh
data. Walk and run are rebuilt as closed cycles. Canonical left-side motions
also produce their bind-relative mirrored runtime counterparts. Missing source
motions are skipped so the pack can be rebuilt incrementally while animating.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
import tempfile

from build_locomotion_cycles import MOTIONS as LOCOMOTION_MOTIONS, build_cycle
from mirror_gait_assets import MIRRORED_MOTIONS, mirrored_glb
from prepare_animation_motion import ANIMATION_FPS, accessor_view, prepare_motion
from prepare_rig_base import GlbError, read_glb


ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIR = ROOT / "assets_src" / "biped" / "unarmed"
RUNTIME_DIR = ROOT / "assets" / "animations" / "biped" / "unarmed"
RUNTIME_BASE = RUNTIME_DIR / "base.glb"

# Keep this table aligned with AnimationPackCatalog::biped_root. Generated
# locomotion and mirrored counterparts are deliberately absent.
DIRECT_MOTIONS = {
    "idle_relaxed": (0,),
    "prone_idle": (0,),
    "supine_idle": (0,),
    "prone_crawl": (0,),
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
}

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
    check: bool = False,
) -> PublicationReport:
    if not runtime_base.is_file():
        raise GlbError(f"runtime MHR base is unavailable: {runtime_base}")

    generated_names = set(LOCOMOTION_MOTIONS) | set(MIRRORED_MOTIONS.values())
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

    published: list[str] = []
    skipped: list[str] = []
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
        prepare_motion(
            source,
            runtime_base,
            runtime_dir / f"{motion}.glb",
            last_frame=max(kept_frames),
            kept_frames=kept_frames,
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
    except (GlbError, OSError, ValueError) as error:
        parser.error(str(error))
    action = "verified" if args.check else "published"
    print(f"{action} {len(report.published)} animation GLBs")
    if report.skipped:
        print("skipped missing sources: " + ", ".join(report.skipped))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
