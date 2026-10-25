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
    def uvclick(u, v):
        x, y = pg.evaluate('cadrust.sketchUV(%s,%s)' % (u, v)); pg.mouse.move(x, y); pg.wait_for_timeout(60); pg.mouse.click(x, y); pg.wait_for_timeout(150)
    def sk(): return pg.evaluate('cadrust.sketch()')
    def solved(): return pg.evaluate('(cadrust.status().sketches.find(s => s.id === cadrust.sketch().feat.id) || {})')
    # --- new sketch on XY +30, four lines, slightly off-axis
    pg.click('button.tool[data-tool=Sketch]'); pg.wait_for_timeout(300)
    pg.select_option('#skPlane', 'XY'); pg.fill('#skOff', '30'); pg.click('#dOk'); pg.wait_for_timeout(600)
    pg.keyboard.press('l')
    for (u, v) in [(70, 0), (100, 2), (101, 20), (69, 21), (70, 0)]: uvclick(u, v)
    pg.wait_for_timeout(600)
    print('entities', [e['type'] for e in sk()['feat']['entities']])
    # --- select tool, coincident at the 4 corners
    pg.keyboard.press('v'); pg.wait_for_timeout(100)
    print('tool', sk()['tool'])
    ents = pg.evaluate('cadrust.sketch().feat.entities')
    def pt(i, which):
        e = ents[i]; return (e['x0'], e['y0']) if which == 'start' else (e['x1'], e['y1'])
    # the snapped endpoints are identical, so clicking a corner selects one of the coincident points; use the panel-free approach: click twice is ambiguous.
    # Instead select by clicking the corner then the other line's point via a tiny offset is impossible; so apply Coincident through the entity-point API: click corner (selects first match), press Escape, and rely on Distance/H/V for the demo. Coincident is exercised below with a separate pair.
    # coincident at the 4 corners: clicking a corner twice selects the two stacked points
    for i in range(4):
        cx, cy = pt(i, 'end')
        uvclick(cx, cy); uvclick(cx, cy); pg.wait_for_timeout(80)
        print('corner', i, 'sel', sk()['sel'], end=' -> ')
        pg.click('button.cbtn[data-con=Coincident]'); pg.wait_for_timeout(700)
        print('cons', len(sk()['feat']['constraints']), 'dof', solved().get('dof'))
    # horizontal / vertical
    def mid(i):
        e = ents[i]; return ((e['x0'] + e['x1']) / 2, (e['y0'] + e['y1']) / 2)
    for i, key in [(0, 'h'), (1, 'v'), (2, 'h'), (3, 'v')]:
        uvclick(*mid(i)); pg.wait_for_timeout(100)
        print('sel', sk()['sel'], end=' -> ')
        pg.keyboard.press(key); pg.wait_for_timeout(700)
        print('cons', len(sk()['feat']['constraints']), 'dof', solved().get('dof'))
    # distance on line 0: select its two endpoints (start of line0 and end of line0 by clicking the corners); corner clicks pick the first point in index order
    ents = solved().get('entities') or ents
    uvclick(ents[0]['x0'], ents[0]['y0']); uvclick(ents[0]['x1'], ents[0]['y1']); pg.wait_for_timeout(100)
    print('sel for distance', sk()['sel'])
    pg.keyboard.press('d'); pg.wait_for_timeout(200)
    print('dim input open', pg.is_visible('#dimInput'))
    pg.fill('#dimInput input', '40'); pg.keyboard.press('Enter'); pg.wait_for_timeout(800)
    # fix first point
    ents = solved().get('entities') or ents
    uvclick(ents[0]['x0'], ents[0]['y0']); pg.keyboard.press('x'); pg.wait_for_timeout(200); pg.keyboard.press('Enter'); pg.wait_for_timeout(800)
    # height dimension on line 1
    ents = solved().get('entities') or ents
    uvclick(ents[1]['x0'], ents[1]['y0']); uvclick(ents[1]['x1'], ents[1]['y1']); pg.keyboard.press('d'); pg.wait_for_timeout(200)
    pg.fill('#dimInput input', '25'); pg.keyboard.press('Enter'); pg.wait_for_timeout(800)
    # coincident: select end of line 1 and start of line 2 by clicking the corner twice would toggle; use entity list clicks instead: click corner (first match), then shift? Simplest: the corner click returns {e:0,p:'end'} first; the second corner click picks the same -> toggles off. So test Coincident via two different corners' matching points is not possible by clicks; test it through constraint buttons after selecting via the debug hook is not allowed. Skip in this run.
    pg.wait_for_timeout(800)
    s = solved(); cons = sk()['feat']['constraints']
    print('constraints', json.dumps(cons))
    print('sketch status', pg.evaluate('cadrust.status().status.find(x => x.id === cadrust.sketch().feat.id)'))
    print('msg', pg.text_content('#msg'))
    if 'entities' not in s: s = {'entities': pg.evaluate('cadrust.sketch().feat.entities'), 'dof': None}
    print('dof', s.get('dof'), 'under', s.get('underconstrained'))
    e0 = s['entities'][0]; print('line0 length', round(((e0['x1'] - e0['x0'])**2 + (e0['y1'] - e0['y0'])**2) ** 0.5, 3), 'y0/y1', round(e0['y0'], 3), round(e0['y1'], 3))
    print('dims labels', pg.locator('#dims .dim').count(), 'con rows', pg.locator('#cons .con').count(), 'dof text', pg.text_content('#skDof'))
    pg.screenshot(path=OUT + 'ui_constraints.png')
    # finish and extrude
    pg.keyboard.press('Enter'); pg.wait_for_timeout(1200)
    skid = pg.evaluate('cadrust.doc().timeline[cadrust.doc().timeline.length-1].id')
    pg.keyboard.press('e'); pg.wait_for_timeout(300)
    opts = pg.evaluate('[...document.querySelectorAll("#dialog select")].map(s => [...s.options].map(o => o.value))')
    for i, o in enumerate(opts):
        if skid in o: pg.locator('#dialog select').nth(i).select_option(skid); break
    pg.click('#dOk'); pg.wait_for_timeout(1500)
    print('bodies', pg.evaluate('cadrust.status().bodies.map(b => [b.id, Math.round(b.volume)])'))
    print('feature errors', [s for s in pg.evaluate('cadrust.status().status') if not s['ok']])
    print('undo stack ok: cards', pg.locator('.feat').count())
    # --- edit the sketch again: constraints loaded?
    pg.locator('.feat').nth(-2).click(); pg.wait_for_timeout(600)
    print('re-entered', sk()['active'], 'cons loaded', len(sk()['feat']['constraints']), 'con rows', pg.locator('#cons .con').count())
    pg.click('#btnCancelSketch'); pg.wait_for_timeout(800)
    print('after cancel active', sk()['active'], 'cards', pg.locator('.feat').count())
    b.close()
print('errors', errors[:5])
