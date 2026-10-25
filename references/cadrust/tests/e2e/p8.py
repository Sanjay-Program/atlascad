"""Dependency graph + incremental evaluation: cache reuse on rebuild, partial recompute on a parameter edit, full reuse on undo, timeline highlight."""
import sys, json, random
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
R = lambda a, b: str(round(random.uniform(a, b), 3))
from playwright.sync_api import sync_playwright
OUT = REPO + 'doc/'
BASE = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:8080/'
errors, fails = [], []
def check(name, cond, info=''):
    print(('PASS' if cond else 'FAIL'), name, info)
    if not cond: fails.append(name)
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1600, 'height': 1000})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' and '404' not in m.text else None)
    pg.goto(BASE + '?theme=dark&lang=en'); pg.wait_for_timeout(4000)
    run = lambda: pg.evaluate('cadrust.lastRun()')
    total = run()['total']
    # the graph: every non-first feature has inputs; the first fillet depends on the base extrude
    g = pg.evaluate('cadrust.graph()')
    check('graph has edges', sum(len(v) for v in g.values()) >= total - 2, json.dumps(g))
    check('fillet depends on extrude', g.get('round_corners') == ['base'], str(g.get('round_corners')))
    # a plain rebuild replays every feature
    pg.click('#btnRun'); pg.wait_for_timeout(1500)
    r = run()
    check('rebuild reuses everything', r['reused'] == total and r['total'] == total, json.dumps(r))
    check('message shows reuse', f'{total}/{total}' in pg.inner_text('#msg'), pg.inner_text('#msg'))
    v0 = pg.evaluate('cadrust.status().bodies[0].volume')
    # parameter edit: only the hole and downstream recompute
    pg.click('#btnParams'); pg.wait_for_timeout(100)
    names = pg.evaluate("[...document.querySelectorAll('#params .pname, #params input')].map(e => e.value || e.textContent)")
    idx = pg.evaluate("[...document.querySelectorAll('#params input')].findIndex(i => i.value === '6.5')")
    check('hole_d field found', idx >= 0, str(names))
    pg.fill(f'#params input >> nth={idx}', R(8, 10)); pg.press(f'#params input >> nth={idx}', 'Enter'); pg.wait_for_timeout(1500)
    r = run()
    check('parameter edit recomputes a subset', 0 < r['reused'] < total, json.dumps(r))
    # base_sk mentions hole_d (the hole circle), so it and everything on the base recompute; the wall branch is reused
    check('independent branch reused, dependent one recomputed', 'wall_sk' in r['cached'] and 'base_sk' not in r['cached'] and 'base' not in r['cached'], str(r['cached']))
    check('volume changed', pg.evaluate('cadrust.status().bodies[0].volume') < v0)
    pg.screenshot(path=OUT + 'ui_incremental.png')
    # undo: the earlier document is still in the cache
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)
    r = run()
    check('undo replays from cache', r['reused'] == total, json.dumps(r))
    pg.click('#btnParamsClose')
    # a late edit: changing the last feature reuses everything before it
    n = total
    pg.click('button.tool[data-tool=Box]'); pg.wait_for_timeout(200); pg.fill('#dialog [data-k=at] input >> nth=0', R(100, 200)); pg.click('#dOk'); pg.wait_for_timeout(1500)
    r = run()
    check('appending a feature reuses all earlier ones', r['reused'] == n and r['total'] == n + 1, json.dumps(r))
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)
    # editing an early feature recomputes its subgraph only
    pg.dblclick('#timeline .feat >> nth=1'); pg.wait_for_timeout(300)
    pg.fill('#dialog [data-k=distance] input', R(6.5, 7.5)); pg.click('#dOk'); pg.wait_for_timeout(2000)
    r = run()
    d = pg.evaluate("cadrust.deps('base')")
    check('early edit keeps independent features cached', r['reused'] >= 1 and set(r['cached']).isdisjoint(set(d['down']) | {'base'}), json.dumps(r) + ' ' + json.dumps(d))
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)
    # hovering a feature highlights its upstream and downstream chain
    pg.hover('#timeline .feat[data-id=base]'); pg.wait_for_timeout(150)
    up = pg.evaluate("[...document.querySelectorAll('#timeline .feat.up')].map(d => d.dataset.id)")
    down = pg.evaluate("[...document.querySelectorAll('#timeline .feat.down')].map(d => d.dataset.id)")
    check('hover highlights the chain', up == ['base_sk'] and len(down) >= 3, str((up, down)))
    pg.screenshot(path=OUT + 'ui_dependency_hover.png')
    pg.mouse.move(10, 500); pg.wait_for_timeout(100)
    check('highlight clears', pg.evaluate("document.querySelectorAll('#timeline .feat.up, #timeline .feat.down').length") == 0)
    check('no console errors', not errors, '\n'.join(errors[:5]))
    b.close()
print('FAILS', fails)
