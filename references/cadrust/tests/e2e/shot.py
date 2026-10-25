import sys, json
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
OUT = os.environ.get('OUT', '/tmp/') 
url = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:8080/?theme=dark&lang=en'
name = sys.argv[2] if len(sys.argv) > 2 else 'shot'
errors = []
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1600, 'height': 1000})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' else None)
    pg.goto(url); pg.wait_for_timeout(4000)
    pg.screenshot(path=OUT + name + '.png')
    for step in sys.argv[3:]:
        kind, _, arg = step.partition(':')
        if kind == 'click': pg.click(arg)
        elif kind == 'key': pg.keyboard.press(arg)
        elif kind == 'eval': print(pg.evaluate(arg))
        elif kind == 'hover': pg.hover(arg)
        elif kind == 'rclick':
            vb = pg.evaluate("(() => { const r = document.querySelector('#viewport canvas.main').getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; })()")
            pg.mouse.click(vb[0] + vb[2] / 2, vb[1] + vb[3] / 2, button='right')
        pg.wait_for_timeout(600)
        pg.screenshot(path=OUT + name + '_' + str(sys.argv.index(step) - 2) + '.png')
    print('status', json.dumps(pg.evaluate('({bodies: cadrust.status().bodies.length, tab: cadrust.tab(), tree: cadrust.tree()})')))
    print('errors', errors[:8])
    b.close()
