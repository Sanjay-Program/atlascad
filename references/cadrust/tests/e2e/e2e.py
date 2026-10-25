"""End-to-end check of the cadrust web app against the real backend on :8080."""
import sys, json
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
OUT = REPO + 'doc/'
errors = []
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1600, 'height': 1000})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' else None)
    pg.goto('http://127.0.0.1:8080/?theme=dark&lang=en'); pg.wait_for_timeout(3000)
    st = pg.evaluate('cadrust.status()')
    print('loaded: cards', pg.locator('.feat').count(), 'edges', len(st['edges']), 'sketches', len(st['sketches']))

    # 1. Fillet with real picked edges: top view, pick the two hole-edge-free top wall edges
    pg.evaluate("cadrust.view('top')"); pg.wait_for_timeout(400)
    pg.click('button.tool[data-tool=Fillet]'); pg.wait_for_timeout(300)
    edges = pg.evaluate('cadrust.status().edges')
    # choose edges whose midpoint lies on the top of the wall (z = 30) and are straight-ish long edges
    top = [e for e in edges if abs(e['mid'][2] - 30) < 1e-3 and len(e['points']) == 2]
    print('top wall edges', len(top))
    picked = 0
    for e in top[:2]:
        x, y = pg.evaluate('cadrust.project(%s)' % json.dumps(e['mid']))
        pg.mouse.move(x, y); pg.wait_for_timeout(80); pg.mouse.click(x, y); pg.wait_for_timeout(150)
        picked += 1
    print('picks text:', pg.text_content('#dialog .picks'))
    pg.fill('#dialog input.expr >> nth=0', '2') if pg.locator('#dialog input.expr').count() else None
    pg.screenshot(path=OUT + 'ui_fillet_pick.png')
    pg.click('#dOk'); pg.wait_for_timeout(1500)
    last = pg.evaluate('cadrust.doc().timeline[cadrust.doc().timeline.length-1]')
    print('fillet feature:', json.dumps(last)[:160])
    print('status last:', pg.evaluate('cadrust.status().status[cadrust.status().status.length-1]'))

    # 2. Sketch on XZ plane at offset -depth... use XY at offset 30 then extrude cut into the wall? simpler: XY offset 0, rectangle + circle, extrude NewBody
    pg.evaluate("cadrust.view('iso')"); pg.wait_for_timeout(300)
    pg.click('button.tool[data-tool=Sketch]'); pg.wait_for_timeout(300)
    pg.select_option('#skPlane', 'XY'); pg.fill('#skOff', '30'); pg.click('#dOk'); pg.wait_for_timeout(600)
    print('sketch mode visible:', pg.is_visible('#sketchbar'))
    def click_world(x, y, z=30):
        sx, sy = pg.evaluate('cadrust.project(%s)' % json.dumps([x, y, z]))
        pg.mouse.move(sx, sy); pg.wait_for_timeout(60); pg.mouse.click(sx, sy); pg.wait_for_timeout(120)
    pg.keyboard.press('r'); click_world(70, 0); click_world(100, 20)
    pg.keyboard.press('c'); click_world(85, 10); click_world(90, 10)
    pg.keyboard.press('l'); click_world(70, 30); click_world(100, 30); click_world(85, 45); click_world(70, 30)
    pg.wait_for_timeout(200)
    pg.screenshot(path=OUT + 'ui_sketch_mode.png')
    ents = pg.evaluate('document.querySelectorAll("#ents .ent, #ents .df, #ents li").length')
    pg.keyboard.press('Enter'); pg.wait_for_timeout(1500)
    sk = pg.evaluate('cadrust.doc().timeline[cadrust.doc().timeline.length-1]')
    print('sketch feature:', sk['type'], sk['id'], [e['type'] for e in sk['entities']])
    stx = pg.evaluate('cadrust.status().sketches')
    print('evaluated sketch loops/open:', [(s['id'], len(s['loops']), len(s['open'])) for s in stx])

    # 3. Extrude it with E
    pg.keyboard.press('e'); pg.wait_for_timeout(300)
    sel = pg.locator('#dialog select').first
    opts = pg.evaluate('[...document.querySelectorAll("#dialog select")].map(s => [...s.options].map(o => o.value))')
    print('extrude dialog selects:', opts)
    # choose the new sketch in the sketch select (first select that contains its id)
    for i, o in enumerate(opts):
        if sk['id'] in o:
            pg.locator('#dialog select').nth(i).select_option(sk['id']); break
    pg.fill('#dialog input.expr >> nth=0', '8') if pg.locator('#dialog input.expr').count() else None
    pg.click('#dOk'); pg.wait_for_timeout(1500)
    bodies = pg.evaluate('cadrust.status().bodies.map(b => [b.id, Math.round(b.volume)])')
    print('bodies after extrude:', bodies)
    errs = [s for s in pg.evaluate('cadrust.status().status') if not s['ok']]
    print('feature errors:', errs)
    pg.click('#btnFit'); pg.wait_for_timeout(300)
    pg.screenshot(path=OUT + 'ui_after_extrude.png')

    # 4. undo twice, redo once
    pg.keyboard.press('Control+z'); pg.wait_for_timeout(900); pg.keyboard.press('Control+z'); pg.wait_for_timeout(900)
    print('cards after 2 undo:', pg.locator('.feat').count())
    pg.keyboard.press('Control+y'); pg.wait_for_timeout(900)
    print('cards after redo:', pg.locator('.feat').count())

    # 5. korean light screenshot
    pg.goto('http://127.0.0.1:8080/?theme=light&lang=ko'); pg.wait_for_timeout(3000)
    pg.screenshot(path=OUT + 'webapp_light_ko.png')
    b.close()
print('errors:', errors[:5])
