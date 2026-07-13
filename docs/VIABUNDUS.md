# Viabundus world data

The strategic world-import pipeline uses **Viabundus Pre-modern Street Map 2**,
version 2 (released 25 April 2025), edited by Bart Holterman et al.

- Source record: <https://doi.org/10.5281/zenodo.16611998>
- Project: <https://www.viabundus.eu>
- License: [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/)

The upstream CSVs are downloaded locally into the Git-ignored `viabundus/`
directory with `just init-viabundus`. The generated strategic graph contains
only the source attributes required to route between settlements in 1544:
nodes, active land/ferry edges, and settlement metadata. It is an adapted
dataset and must retain this attribution and CC BY-SA 4.0 licensing when
distributed.

Each imported settlement has the prototype's shared merchant services, and
newly created characters start at a random loaded settlement.

The import does not claim that every represented line is an exact historical
road. Viabundus' `certainty` value is preserved on each travel edge so gameplay
and presentation can account for uncertain reconstructions later.
