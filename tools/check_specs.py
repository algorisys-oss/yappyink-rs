#!/usr/bin/env python3
"""Validate specification references only. This does not test the application."""
from __future__ import annotations
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]

def read_json(name: str) -> dict:
    return json.loads((ROOT / name).read_text(encoding='utf-8'))

def unique(items: list[str], description: str) -> None:
    if len(items) != len(set(items)):
        raise ValueError(f'Duplicate {description}')

def main() -> int:
    reqs = read_json('requirements.json')['requirements']
    tasks = read_json('tasks.json')['tasks']
    trace = read_json('traceability.json')
    scenarios = trace['scenarios']
    req_ids = [r['id'] for r in reqs]
    task_ids = [t['id'] for t in tasks]
    scenario_ids = [s['id'] for s in scenarios]
    unique(req_ids, 'requirement IDs')
    unique(task_ids, 'task IDs')
    unique(scenario_ids, 'scenario IDs')
    rset, tset, sset = set(req_ids), set(task_ids), set(scenario_ids)
    graph = {t['id']: t['depends_on'] for t in tasks}
    for task in tasks:
        if not set(task['requirements']) <= rset:
            raise ValueError(f"Unknown requirements in {task['id']}")
        if not set(task['depends_on']) <= tset:
            raise ValueError(f"Unknown dependencies in {task['id']}")
        if task['id'] in task['depends_on']:
            raise ValueError(f"Self-dependency in {task['id']}")
    visiting, visited = set(), set()
    def visit(task_id: str) -> None:
        if task_id in visiting:
            raise ValueError(f'Dependency cycle at {task_id}')
        if task_id in visited:
            return
        visiting.add(task_id)
        for dependency in graph[task_id]:
            visit(dependency)
        visiting.remove(task_id)
        visited.add(task_id)
    for task_id in graph:
        visit(task_id)
    for scenario in scenarios:
        if not set(scenario['requirements']) <= rset:
            raise ValueError(f"Unknown scenario requirement in {scenario['id']}")
        path = ROOT / scenario['file']
        if not path.is_file():
            raise ValueError(f'Missing scenario file: {path}')
        if f"Scenario: [{scenario['id']}]" not in path.read_text(encoding='utf-8'):
            raise ValueError(f"Missing scenario declaration for {scenario['id']}")
    seen = set()
    for entry in trace['requirements']:
        rid = entry['requirement']
        if rid not in rset or rid in seen:
            raise ValueError(f'Unknown or duplicate trace requirement: {rid}')
        seen.add(rid)
        expected_tasks = {t['id'] for t in tasks if rid in t['requirements']}
        expected_scenarios = {s['id'] for s in scenarios if rid in s['requirements']}
        if not entry['tasks'] or set(entry['tasks']) != expected_tasks:
            raise ValueError(f'Incorrect or empty task mapping for {rid}')
        if not entry['scenarios'] or set(entry['scenarios']) != expected_scenarios:
            raise ValueError(f'Incorrect or empty scenario mapping for {rid}')
        if not set(entry['tasks']) <= tset or not set(entry['scenarios']) <= sset:
            raise ValueError(f'Unresolved trace references for {rid}')
    if seen != rset:
        raise ValueError('Traceability does not cover every requirement')
    sources = (ROOT / 'sources.md').read_text(encoding='utf-8')
    known_sources = set(re.findall(r'^## (S\d{2}):', sources, flags=re.M))
    for path in ROOT.rglob('*.md'):
        text = path.read_text(encoding='utf-8')
        if not text.strip():
            raise ValueError(f'Empty Markdown file: {path}')
        fences = sum(1 for line in text.splitlines() if line.startswith('```'))
        if fences % 2:
            raise ValueError(f'Unbalanced code fences: {path}')
        referenced = set(re.findall(r'\bS\d{2}\b', text))
        if not referenced <= known_sources:
            raise ValueError(f'Unknown source ID in {path}: {referenced-known_sources}')
    all_declarations = []
    for path in (ROOT / 'tests' / 'acceptance').glob('*.feature'):
        all_declarations.extend(re.findall(r'Scenario: \[(AC-[A-Z]+-\d{3})\]', path.read_text()))
    unique(all_declarations, 'scenario declarations')
    if set(all_declarations) != sset:
        raise ValueError('Scenario files and traceability index disagree')
    print(f'PASS: {len(reqs)} unique requirements; {len(tasks)} tasks; {len(scenarios)} scenario specifications.')
    print('PASS: dependency graph is acyclic; task/scenario mappings resolve; all requirements have coverage.')
    print('PASS: source IDs resolve; Markdown code fences are balanced; scenario files exist.')
    print('NOT TESTED: Rust code, desktop overlays, native APIs, application behavior, permissions, or performance.')
    return 0

if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (KeyError, ValueError, OSError, json.JSONDecodeError) as exc:
        print(f'FAIL: {exc}', file=sys.stderr)
        raise SystemExit(1)
