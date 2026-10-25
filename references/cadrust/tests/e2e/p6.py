"""Fusion-style chrome: ribbon tabs and group menus, browser visibility, timeline marker, data panel, viewcube, nav modes."""
import sys, json
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
OUT = REPO + 'doc/'
errors, fails = [], []
def check(name, cond, info=''):
    print(('PASS' if cond else 'FAIL'), name, info)
    if not cond: fails.append(name)
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1600, 'height': 1000})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' else None)
    pg.goto('http://127.0.0.1:8080/?theme=dark&lang=en'); pg.wait_for_timeout(3500)
    check('design tab shows the file name', pg.inner_text('#docName') == 'bracket', pg.inner_text('#docName'))
    check('solid tab active', pg.evaluate('cadrust.tab()') == ['solid'])
    tree = pg.evaluate('cadrust.tree()')
    check('browser tree has bodies and sketches', any(t.startswith('▾Bodies') for t in tree) and 'base' in tree and 'base_sk' in tree, str(tree))
    pg.screenshot(path=OUT + 'webapp_dark_en.png')

    # ribbon group menu lists the commands with shortcuts
    pg.click('.rlabel[data-menu=create]'); pg.wait_for_timeout(150)
    items = pg.evaluate("[...document.querySelectorAll('#groupMenu .mi')].map(d => d.textContent.trim())")
    check('create menu lists commands', any(i.startswith('Extrude') and i.endswith('E') for i in items) and len(items) >= 10, str(items))
    pg.click('#groupMenu .mi >> text=Extrude'); pg.wait_for_timeout(300)
    check('menu item starts the tool', pg.is_visible('#dialog') and 'EXTRUDE' in pg.inner_text('#dialog .dh').upper(), pg.inner_text('#dialog .dh'))
    pg.click('#dCancel')

    # tabs
    pg.click('.rtab[data-tab=util]'); pg.wait_for_timeout(100)
    check('utilities tab', pg.evaluate('cadrust.tab()') == ['util'] and pg.is_visible('#btnStl'))
    pg.click('.rtab[data-tab=solid]')

    # pattern submenu
    pg.click('#btnPatternMenu'); pg.click('#patternMenu [data-tool=CircularPattern]'); pg.wait_for_timeout(300)
    check('pattern submenu opens circular pattern', 'CIRCULAR' in pg.inner_text('#dialog .dh').upper(), pg.inner_text('#dialog .dh'))
    pg.click('#dCancel')

    # browser eye hides a body (meshes + edges)
    pg.click('#tree li.d2 .eye >> nth=0'); pg.wait_for_timeout(200)
    hid = pg.evaluate('cadrust.hidden()')
    vis = pg.evaluate("(() => { let m = 0, v = 0; cadrust.model().forEach(([t,,,vis]) => { if (t === 'Mesh') { m++; if (vis) v++; } }); return [m, v]; })()")
    check('hide body via browser', hid == ['base'] and vis[0] > 0 and vis[1] == 0, str((hid, vis)))
    pg.click('#tree li.d2 .eye >> nth=0'); pg.wait_for_timeout(200)
    check('show body again', pg.evaluate('cadrust.hidden()') == [])

    # Browser actions: named views move the camera, origin planes select / look at / start a sketch,
    # bodies zoom on double click, sketches select and edit from the tree
    pg.click('#tree li.d1:has-text("Named Views") .tw'); pg.wait_for_timeout(150)
    pg.click('#tree li.view:has-text("Top")'); pg.wait_for_timeout(500)
    cam = pg.evaluate('cadrust.camera()')
    check('named view Top looks down z', abs(cam[0]) < 1e-3 and abs(cam[1]) < 1e-3 and cam[2] > 0.99, str(cam))
    pg.click('#tree li.view:has-text("Front")'); pg.wait_for_timeout(500)
    check('named view Front looks along +y', pg.evaluate('cadrust.camera()')[1] < -0.99)
    pg.click('#tree li.view:has-text("Home")'); pg.wait_for_timeout(500)
    pg.click('#tree li.d1:has-text("Origin") .tw'); pg.wait_for_timeout(150)
    pg.click('#tree li.plane:has-text("XZ")'); pg.wait_for_timeout(150)
    check('origin plane selected and shown', pg.evaluate('cadrust.selectedPlane()') == 'XZ' and pg.evaluate("cadrust.planeVisible('XZ')") and not pg.evaluate("cadrust.planeVisible('XY')"))
    pg.click('#tree li.plane:has-text("XZ")', button='right'); pg.wait_for_timeout(150)
    pg.click('#ctx >> text=Look At'); pg.wait_for_timeout(500)
    check('look at XZ plane', pg.evaluate('cadrust.camera()')[1] < -0.99, str(pg.evaluate('cadrust.camera()')))
    pg.dblclick('#tree li.plane:has-text("YZ")'); pg.wait_for_timeout(800)
    sk = pg.evaluate('cadrust.sketch()')
    check('double-click on a plane starts a sketch on it', sk['active'] and sk['feat']['plane'] == 'YZ', json.dumps(sk['feat']))
    pg.click('#btnCancelSketch'); pg.wait_for_timeout(800)
    check('cancelled new sketch leaves the timeline as before', len(pg.evaluate('cadrust.doc().timeline')) == 10)
    pg.evaluate("cadrust.view('iso')"); pg.wait_for_timeout(100)
    pg.dblclick('#tree li.d2:has-text("base")'); pg.wait_for_timeout(600)
    tgt = pg.evaluate('cadrust.target()')
    bb = pg.evaluate('cadrust.status().bodies[0]')
    cen = [(bb['bbox_min'][k] + bb['bbox_max'][k]) / 2 for k in range(3)]
    check('double-click on a body zooms to it', all(abs(tgt[k] - cen[k]) < 1 for k in range(3)), str((tgt, cen)))
    pg.click('#tree li.d2:has-text("wall_sk")'); pg.wait_for_timeout(200)
    check('click selects the sketch', pg.evaluate('cadrust.selectedSketch()') == 'wall_sk' and pg.evaluate("document.querySelectorAll('#timeline .feat.self').length") >= 1)
    pg.dblclick('#tree li.d2:has-text("base_sk")'); pg.wait_for_timeout(800)
    pg.dblclick('#tree li.d2:has-text("wall_sk")'); pg.wait_for_timeout(800)
    sk = pg.evaluate('cadrust.sketch()')
    check('double-click switches to another sketch while editing', sk['active'] and sk['feat']['id'] == 'wall_sk')
    pg.click('#btnCancelSketch'); pg.wait_for_timeout(800)
    check('timeline intact after switching sketches', len(pg.evaluate('cadrust.doc().timeline')) == 10 and not [x for x in pg.evaluate('cadrust.status().status') if not x['ok']])
    pg.click('#tree li.d2:has-text("wall_sk")'); pg.wait_for_timeout(100)

    # the eye on a sketch hides its loops only; the model and the dependent features are untouched
    n_loops = pg.evaluate('cadrust.model ? 0 : 0')
    pg.click('#tree li:has-text("base_sk") .eye'); pg.wait_for_timeout(300)
    check('sketch eye hides the sketch, not the model', pg.evaluate('cadrust.hiddenSketches()') == ['base_sk'] and pg.evaluate('cadrust.status().bodies.length') == 1 and not [s for s in pg.evaluate('cadrust.status().status') if not s['ok']] and not pg.evaluate('cadrust.doc().suppressed'))
    pg.dblclick('#tree li:has-text("base_sk")'); pg.wait_for_timeout(800)
    check('hidden sketch can still be edited', pg.evaluate('cadrust.sketch()')['active'] and pg.evaluate('cadrust.sketch()')['feat']['id'] == 'base_sk')
    pg.click('#btnCancelSketch'); pg.wait_for_timeout(800)
    pg.click('#tree li:has-text("base_sk") .eye'); pg.wait_for_timeout(200)
    check('sketch shown again', pg.evaluate('cadrust.hiddenSketches()') == [])
    pg.click('#tree li.d1:has-text("Sketches") .eye'); pg.wait_for_timeout(200)
    check('group eye hides every sketch', sorted(pg.evaluate('cadrust.hiddenSketches()')) == ['base_sk', 'wall_sk'] and pg.evaluate('cadrust.status().bodies.length') == 1)
    pg.click('#tree li.d1:has-text("Sketches") .eye'); pg.wait_for_timeout(200)

    # timeline history marker: roll back two features, new feature inserts at the marker, roll to end
    n = len(pg.evaluate('cadrust.doc().timeline'))
    pg.click('#tlPrev'); pg.wait_for_timeout(1200); pg.click('#tlPrev'); pg.wait_for_timeout(1200)
    r = pg.evaluate('cadrust.roll()')
    check('marker rolled back', r['index'] == n - 2 and r['sent'] == n - 2, str(r))
    ids_sent = [s['id'] for s in pg.evaluate('cadrust.status().status')]
    check('status only has computed features', len(ids_sent) == n - 2, str(len(ids_sent)))
    pg.screenshot(path=OUT + 'ui_timeline_marker.png')
    pg.keyboard.press('e'); pg.wait_for_timeout(300)
    pg.evaluate("document.activeElement.blur()"); pg.keyboard.press('Enter'); pg.wait_for_timeout(1200)
    tl = pg.evaluate('cadrust.doc().timeline')
    check('new feature inserted at the marker', len(tl) == n + 1 and tl[n - 2]['type'] == 'Extrude', str([f['id'] for f in tl]))
    pg.click('#btnUndo'); pg.wait_for_timeout(1000)
    pg.click('#tlLast'); pg.wait_for_timeout(1200)
    r = pg.evaluate('cadrust.roll()')
    check('marker back at end', r['roll'] is None and r['sent'] == n, str(r))
    # context menu roll
    pg.click('#timeline .feat >> nth=2', button='right'); pg.wait_for_timeout(100)
    pg.click('#ctx >> text=Roll history marker here'); pg.wait_for_timeout(1200)
    check('roll here from context menu', pg.evaluate('cadrust.roll().index') == 3)
    pg.click('#tlLast'); pg.wait_for_timeout(1200)

    # data panel lists designs next to the served document and opens one
    pg.click('#btnData'); pg.wait_for_timeout(400)
    names = pg.evaluate("[...document.querySelectorAll('#dpList .ex .nm')].map(d => d.textContent)")
    check('data panel lists designs', 'bracket' in names and 'phase2' in names, str(names))
    pg.click('#dpList .ex >> text=phase2'); pg.wait_for_timeout(2500)
    check('open design switches document', pg.inner_text('#docName') == 'phase2' and pg.evaluate('cadrust.status().bodies.length') >= 1, pg.inner_text('#docName'))
    pg.click('#btnData'); pg.click('#dpList .ex >> text=bracket'); pg.wait_for_timeout(2500)
    check('back to bracket', pg.inner_text('#docName') == 'bracket')

    # viewcube click on the TOP face changes the view direction
    before = pg.evaluate('[...cadrust.camera()]')
    vb = pg.evaluate("(() => { const r = document.querySelector('#vcCanvas').getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; })()")
    pg.mouse.click(vb[0] + vb[2] * 0.56, vb[1] + vb[3] * 0.25); pg.wait_for_timeout(500)
    after = pg.evaluate('[...cadrust.camera()]')
    check('viewcube top face looks down z', abs(after[0]) < 1e-3 and abs(after[1]) < 1e-3 and after[2] > 0.99, str((before, after)))
    pg.evaluate("cadrust.view('iso')")

    # navigation bar pan mode changes the left-drag binding
    pg.click('#navbar [data-nav=pan]'); check('pan mode', pg.evaluate('cadrust.navMode()') == 'pan')
    pg.click('#navbar [data-nav=orbit]')

    # right click: the marking menu opens and the browser's own context menu is suppressed, also over the menu itself
    vb = pg.evaluate("(() => { const r = document.querySelector('#viewport canvas.main').getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; })()")
    pg.mouse.click(vb[0] + vb[2] / 2, vb[1] + vb[3] / 2, button='right'); pg.wait_for_timeout(200)
    check('marking menu opens on right click', len(pg.evaluate('cadrust.mark()')) == 8, str(pg.evaluate('cadrust.mark()')))
    prevented = pg.evaluate("[document.querySelector('#mark'), document.querySelector('#viewport canvas.main'), document.querySelector('#timeline')].map(el => { const e = new MouseEvent('contextmenu', { bubbles: true, cancelable: true }); el.dispatchEvent(e); return e.defaultPrevented; })")
    check('browser context menu suppressed over the menu, canvas and timeline', prevented == [True, True, True], str(prevented))
    check('text fields keep the native menu', pg.evaluate("(() => { const i = document.querySelector('#lang') || document.querySelector('input'); const e = new MouseEvent('contextmenu', { bubbles: true, cancelable: true }); document.querySelector('#newPName').dispatchEvent(e); return !e.defaultPrevented; })()"))
    pg.keyboard.press('Escape'); pg.wait_for_timeout(100)

    # file menu + help menu open/close, rich tooltip
    pg.click('#btnFile'); check('file menu opens', pg.is_visible('#fileMenu')); pg.keyboard.press('Escape'); check('Esc closes menu', not pg.is_visible('#fileMenu'))
    pg.hover('button.tool[data-tool=Fillet]'); pg.wait_for_timeout(150)
    check('rich tooltip', pg.is_visible('#tip') and pg.inner_text('#tip .tt').startswith('Fillet'), pg.inner_text('#tip'))

    # change parameters panel edits a parameter and rebuilds
    pg.click('#btnParams'); pg.wait_for_timeout(100)
    v0 = pg.evaluate("cadrust.status().bodies[0].volume")
    pg.fill('#params input >> nth=1', '50'); pg.press('#params input >> nth=1', 'Enter'); pg.wait_for_timeout(1500)
    v1 = pg.evaluate("cadrust.status().bodies[0].volume")
    check('parameter edit rebuilds', abs(v1 - v0) > 1, str((v0, v1)))
    pg.click('#btnUndo'); pg.wait_for_timeout(1000); pg.click('#btnParamsClose')

    # sketch tab appears in sketch mode and goes away after
    pg.click('button.tool[data-tool=Sketch]'); pg.click('#dOk'); pg.wait_for_timeout(600)
    check('sketch contextual tab', pg.evaluate('cadrust.tab()') == ['sketch'] and pg.is_visible('#sketchbar') and pg.is_visible('#skpanel'))
    pg.screenshot(path=OUT + 'ui_sketch_mode.png')
    pg.click('#btnCancelSketch'); pg.wait_for_timeout(600)
    check('back to solid tab', pg.evaluate('cadrust.tab()') == ['solid'] and not pg.is_visible('#sketchbar'))

    # light theme + korean screenshot
    pg.select_option('#lang', 'ko'); pg.click('#btnTheme'); pg.wait_for_timeout(400)
    check('ko ribbon labels', pg.inner_text('.rtab[data-tab=solid]') == '솔리드' and pg.inner_text('.rlabel[data-menu=modify]').startswith('수정'))
    pg.screenshot(path=OUT + 'webapp_light_ko.png')
    pg.select_option('#lang', 'en'); pg.click('#btnTheme')

    check('no console errors', not errors, '\n'.join(errors[:5]))
    b.close()
print('FAILS', fails)
