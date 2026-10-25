"""Themed scrollbars and splitters: drag the Browser, Data Panel and right stack edges; sizes persist; scrollbars use theme tokens."""
import sys, json
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
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
    pg.goto(BASE + '?theme=dark&lang=en'); pg.wait_for_timeout(3500)
    pg.evaluate("localStorage.removeItem('cadrust.ui')")
    def drag(sel, dx, dy):
        r = pg.evaluate(f"(() => {{ const r = document.querySelector('{sel}').getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; }})()")
        pg.mouse.move(r[0], r[1]); pg.mouse.down(); pg.mouse.move(r[0] + dx / 2, r[1] + dy / 2); pg.mouse.move(r[0] + dx, r[1] + dy); pg.mouse.up(); pg.wait_for_timeout(150)
    s0 = pg.evaluate('cadrust.panelSizes()')
    # scrollbar colours come from the theme tokens
    sc = pg.evaluate("getComputedStyle(document.querySelector('#tree')).scrollbarColor")
    tok = pg.evaluate("getComputedStyle(document.documentElement).getPropertyValue('--sb').trim()")
    check('scrollbar colour uses theme tokens', tok and tok.lower() in sc.lower().replace(' ', '') or 'rgb' in sc, f'{sc} / {tok}')
    check('ribbon hides its scrollbar', pg.evaluate("getComputedStyle(document.querySelector('.rpanel')).scrollbarWidth") == 'none')
    # browser splitter: wider and taller
    drag('#browser .sp-e', 120, 0)
    s1 = pg.evaluate('cadrust.panelSizes()')
    check('browser widened by drag', abs(s1['browser'] - (s0['browser'] + 120)) < 2, f'{s0["browser"]} -> {s1["browser"]}')
    h0 = pg.evaluate("document.querySelector('#browser').getBoundingClientRect().height")
    drag('#browser .sp-s', 0, 80)
    h1 = pg.evaluate("document.querySelector('#browser').getBoundingClientRect().height")
    check('browser made taller by drag', abs(h1 - (h0 + 80)) < 2, f'{h0} -> {h1}')
    # right stack: open a dialog and drag its left edge
    pg.keyboard.press('e'); pg.wait_for_timeout(300)
    w0 = pg.evaluate("document.querySelector('#dialog').getBoundingClientRect().width")
    drag('#rstack .sp-w', -100, 0)
    w1 = pg.evaluate("document.querySelector('#dialog').getBoundingClientRect().width")
    check('dialog stack widened by drag', abs(w1 - (w0 + 100)) < 2, f'{w0} -> {w1}')
    pg.screenshot(path=OUT + 'ui_splitters.png')
    pg.click('#dCancel')
    # data panel
    pg.click('#btnData'); pg.wait_for_timeout(300)
    d0 = pg.evaluate("document.querySelector('#datapanel').getBoundingClientRect().width")
    drag('#datapanel .sp-e', 90, 0)
    d1 = pg.evaluate("document.querySelector('#datapanel').getBoundingClientRect().width")
    check('data panel widened by drag', abs(d1 - (d0 + 90)) < 2, f'{d0} -> {d1}')
    pg.click('#btnDataClose')
    # sizes persist across reloads
    pg.reload(); pg.wait_for_timeout(3000)
    s2 = pg.evaluate('cadrust.panelSizes()')
    check('sizes persist after reload', abs(s2['browser'] - s1['browser']) < 2 and abs(s2['rstack'] - (w0 + 100)) < 2 and abs(s2['stored']['datapanel']['w'] - d1) < 2, json.dumps(s2))
    # minimum width is enforced
    drag('#browser .sp-e', -600, 0)
    check('browser min width', abs(pg.evaluate('cadrust.panelSizes()')['browser'] - 180) < 2)
    # light theme switches the scrollbar palette too
    pg.click('#btnTheme'); pg.wait_for_timeout(200)
    tok2 = pg.evaluate("getComputedStyle(document.documentElement).getPropertyValue('--sb').trim()")
    check('light theme has its own scrollbar colour', tok2 != tok, f'{tok} vs {tok2}')
    pg.evaluate("localStorage.removeItem('cadrust.ui')")
    check('no console errors', not errors, '\n'.join(errors[:5]))
    b.close()
print('FAILS', fails)
