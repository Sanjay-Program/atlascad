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
    pg.goto('http://127.0.0.1:8080/?theme=light&lang=ko'); pg.wait_for_timeout(3000)
    vol0 = pg.evaluate('cadrust.status().bodies[0].volume')
    pg.click('button.tool[data-tool=Sketch]'); pg.wait_for_timeout(300)
    # face picking is armed as soon as the dialog opens; one click on the face starts the sketch
    x, y = pg.evaluate('cadrust.project([15, 10, 6])')   # top face of the bracket base (z = thickness)
    pg.mouse.move(x, y); pg.wait_for_timeout(100); pg.mouse.click(x, y); pg.wait_for_timeout(200)
    print('sketch started on the face:', pg.evaluate('cadrust.sketch().active'), pg.evaluate('JSON.stringify(cadrust.sketch().feat.plane)'))
    if pg.is_visible('#dialog #dOk'): pg.click('#dOk')
    pg.wait_for_timeout(1500)
    sk = pg.evaluate('cadrust.sketch()'); print('plane:', sk['feat']['plane'])
    info = pg.evaluate('cadrust.status().sketches.find(s => s.id === cadrust.sketch().feat.id)')
    print('basis origin', [round(v, 2) for v in info['origin']], 'normal', [round(v, 2) for v in info['normal']], 'u', [round(v, 2) for v in info['u']])
    print('panel header:', pg.text_content('#skpanel h3'))
    # draw a circle on the face: centre at sketch (5, 5), radius 4
    pg.keyboard.press('c')
    for (u, v) in [(-20, 8), (-16, 8)]:
        sx, sy = pg.evaluate('cadrust.sketchUV(%s,%s)' % (u, v)); pg.mouse.move(sx, sy); pg.wait_for_timeout(60); pg.mouse.click(sx, sy); pg.wait_for_timeout(150)
    pg.wait_for_timeout(700)
    print('entities', pg.evaluate('cadrust.sketch().feat.entities'))
    pg.screenshot(path=OUT + 'ui_face_sketch.png')
    pg.keyboard.press('Enter'); pg.wait_for_timeout(1200)
    skid = pg.evaluate('cadrust.doc().timeline[cadrust.doc().timeline.length-1].id')
    pg.keyboard.press('e'); pg.wait_for_timeout(300)
    opts = pg.evaluate('[...document.querySelectorAll("#dialog select")].map(s => [...s.options].map(o => o.value))')
    for i, o in enumerate(opts):
        if skid in o: pg.locator('#dialog select').nth(i).select_option(skid); break
    pg.locator('#dialog select').nth(1).select_option('Join'); pg.wait_for_timeout(100)
    pg.locator('#dialog select').nth(2).select_option('base')
    pg.click('#dOk'); pg.wait_for_timeout(1500)
    print('bodies', pg.evaluate('cadrust.status().bodies.map(b => [b.id, Math.round(b.volume)])'), 'was', round(vol0))
    print('feature errors', [s for s in pg.evaluate('cadrust.status().status') if not s['ok']])
    pg.screenshot(path=OUT + 'ui_face_extrude.png')
    b.close()
print('errors', errors[:5])
