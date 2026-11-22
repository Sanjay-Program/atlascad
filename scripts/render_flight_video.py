#!/usr/bin/env python3
"""
AtlasCAD Supersonic Mechanical Aircraft 6-DOF Flight Video Generator
Renders a 60 FPS / 30 FPS Full HD/720p MP4 demonstration of the 3D supersonic aircraft
flying with US Standard Atmosphere 1976 physics, afterburner shock diamonds,
high-G bank maneuvers, transonic shock cone, and live AtlasCAD tactical HUD.
"""

import math
import os
import subprocess
import sys
import numpy as np
from PIL import Image, ImageDraw, ImageFont

# ============================================================================
# 3D AIRCRAFT GEOMETRY DEFINITION (Matches AtlasCAD buildSupersonicAircraftMesh)
# ============================================================================

def get_aircraft_geometry():
    # Vertices (meters): Length ~17.5m, Wingspan ~12.4m, Height ~3.1m
    # Coordinate system: X = right, Y = forward (nose +), Z = up
    vertices = {
        "nose": np.array([0.0, 9.0, 0.2]),
        "cockpit_top": np.array([0.0, 4.0, 1.3]),
        "cockpit_l": np.array([-0.7, 3.5, 0.4]),
        "cockpit_r": np.array([0.7, 3.5, 0.4]),
        "nose_bot": np.array([0.0, 4.0, -0.4]),

        "canard_l": np.array([-2.2, 4.2, 0.4]),
        "canard_r": np.array([2.2, 4.2, 0.4]),
        "canard_te_l": np.array([-2.0, 3.2, 0.35]),
        "canard_te_r": np.array([2.0, 3.2, 0.35]),

        "mid_top": np.array([0.0, -1.0, 1.2]),
        "mid_l": np.array([-1.3, -1.0, 0.3]),
        "mid_r": np.array([1.3, -1.0, 0.3]),
        "mid_bot": np.array([0.0, -1.0, -0.5]),

        "aft_top": np.array([0.0, -6.5, 0.9]),
        "aft_l": np.array([-1.4, -6.5, 0.2]),
        "aft_r": np.array([1.4, -6.5, 0.2]),
        "aft_bot": np.array([0.0, -6.5, -0.4]),

        "noz_l": np.array([-0.7, -8.5, 0.1]),
        "noz_r": np.array([0.7, -8.5, 0.1]),
        "noz_top": np.array([0.0, -8.3, 0.5]),
        "noz_bot": np.array([0.0, -8.3, -0.3]),

        # Left Swept Delta Wing
        "w_root_le_l": np.array([-1.3, 2.5, 0.2]),
        "w_root_te_l": np.array([-1.4, -5.0, 0.1]),
        "w_tip_le_l": np.array([-6.2, -3.5, 0.3]),
        "w_tip_te_l": np.array([-5.8, -4.8, 0.2]),
        "w_root_bot_l": np.array([-1.3, -1.0, -0.1]),
        "w_tip_bot_l": np.array([-6.0, -4.1, 0.1]),

        # Right Swept Delta Wing
        "w_root_le_r": np.array([1.3, 2.5, 0.2]),
        "w_root_te_r": np.array([1.4, -5.0, 0.1]),
        "w_tip_le_r": np.array([6.2, -3.5, 0.3]),
        "w_tip_te_r": np.array([5.8, -4.8, 0.2]),
        "w_root_bot_r": np.array([1.3, -1.0, -0.1]),
        "w_tip_bot_r": np.array([6.0, -4.1, 0.1]),

        # Left Canted Vertical Stabilizer
        "t_root_le_l": np.array([-1.0, -3.5, 0.8]),
        "t_root_te_l": np.array([-1.2, -6.5, 0.7]),
        "t_tip_le_l": np.array([-1.6, -5.8, 2.6]),
        "t_tip_te_l": np.array([-1.8, -7.0, 2.4]),

        # Right Canted Vertical Stabilizer
        "t_root_le_r": np.array([1.0, -3.5, 0.8]),
        "t_root_te_r": np.array([1.2, -6.5, 0.7]),
        "t_tip_le_r": np.array([1.6, -5.8, 2.6]),
        "t_tip_te_r": np.array([1.8, -7.0, 2.4]),
    }

    # Faces defined by vertex keys, color category: 'fuselage', 'canopy', 'wing', 'fin', 'nozzle'
    faces = [
        # Nose cone & cockpit
        (["nose", "cockpit_l", "cockpit_top"], "canopy"),
        (["nose", "cockpit_top", "cockpit_r"], "canopy"),
        (["nose", "nose_bot", "cockpit_l"], "fuselage"),
        (["nose", "cockpit_r", "nose_bot"], "fuselage"),

        # Canards
        (["cockpit_l", "canard_l", "canard_te_l"], "wing"),
        (["cockpit_r", "canard_te_r", "canard_r"], "wing"),

        # Mid Fuselage
        (["cockpit_top", "cockpit_l", "mid_l", "mid_top"], "fuselage"),
        (["cockpit_top", "mid_top", "mid_r", "cockpit_r"], "fuselage"),
        (["nose_bot", "mid_bot", "mid_l", "cockpit_l"], "fuselage_under"),
        (["nose_bot", "cockpit_r", "mid_r", "mid_bot"], "fuselage_under"),

        # Aft Fuselage
        (["mid_top", "mid_l", "aft_l", "aft_top"], "fuselage"),
        (["mid_top", "aft_top", "aft_r", "mid_r"], "fuselage"),
        (["mid_bot", "aft_bot", "aft_l", "mid_l"], "fuselage_under"),
        (["mid_bot", "mid_r", "aft_r", "aft_bot"], "fuselage_under"),

        # Exhaust Fairings
        (["aft_top", "aft_l", "noz_l"], "fuselage"),
        (["aft_top", "noz_r", "aft_r"], "fuselage"),
        (["aft_bot", "noz_l", "aft_l"], "fuselage_under"),
        (["aft_bot", "aft_r", "noz_r"], "fuselage_under"),
        (["aft_top", "noz_l", "noz_r", "aft_top"], "nozzle"),
        (["aft_bot", "noz_r", "noz_l", "aft_bot"], "nozzle"),

        # Left Delta Wing
        (["w_root_le_l", "w_root_te_l", "w_tip_te_l", "w_tip_le_l"], "wing"),
        (["w_root_le_l", "w_tip_le_l", "w_tip_bot_l", "w_root_bot_l"], "wing"),
        (["w_root_bot_l", "w_tip_bot_l", "w_tip_te_l", "w_root_te_l"], "wing_under"),

        # Right Delta Wing
        (["w_root_le_r", "w_tip_le_r", "w_tip_te_r", "w_root_te_r"], "wing"),
        (["w_root_le_r", "w_root_bot_r", "w_tip_bot_r", "w_tip_le_r"], "wing"),
        (["w_root_bot_r", "w_root_te_r", "w_tip_te_r", "w_tip_bot_r"], "wing_under"),

        # Left Vertical Fin
        (["t_root_le_l", "t_root_te_l", "t_tip_te_l", "t_tip_le_l"], "fin"),
        (["t_root_le_l", "t_tip_le_l", "t_tip_te_l", "t_root_te_l"], "fin"),

        # Right Vertical Fin
        (["t_root_le_r", "t_tip_le_r", "t_tip_te_r", "t_root_te_r"], "fin"),
        (["t_root_le_r", "t_root_te_r", "t_tip_te_r", "t_tip_le_r"], "fin"),
    ]

    return vertices, faces

# Color palette for mechanical 3D rendering
PALETTE = {
    "fuselage": (52, 68, 88),         # Metallic titanium blue-slate
    "fuselage_under": (38, 50, 68),   # Darker belly
    "canopy": (70, 150, 210),         # Reflective gold-tinted cockpit glass
    "wing": (48, 64, 84),             # Carbon composite upper skin
    "wing_under": (34, 46, 62),       # Dark lower surface
    "fin": (56, 74, 98),              # Stabilizer panels
    "nozzle": (25, 30, 40),           # Heat-treated inconel exhaust
    "wireframe": (56, 189, 248, 180), # Cyan wireframe line
    "hud_cyan": (34, 211, 238),       # Primary HUD bright cyan
    "hud_green": (16, 185, 129),      # HUD green status
    "hud_amber": (245, 158, 11),      # Warning amber
    "hud_red": (239, 68, 68),         # Danger red
    "ab_orange": (255, 120, 20),      # Afterburner core flame
    "ab_cyan": (120, 240, 255),       # Shock diamond plasma
}

# ============================================================================
# 3D TRANSFORMATION & RENDERING ENGINE
# ============================================================================

def make_rotation_matrix(roll_deg, pitch_deg, yaw_deg):
    r = math.radians(roll_deg)
    p = math.radians(pitch_deg)
    y = math.radians(yaw_deg)

    # Roll around Y (forward axis in our CAD coordinate system)
    R_roll = np.array([
        [math.cos(r), 0, math.sin(r)],
        [0, 1, 0],
        [-math.sin(r), 0, math.cos(r)]
    ])

    # Pitch around X (lateral axis)
    R_pitch = np.array([
        [1, 0, 0],
        [0, math.cos(p), -math.sin(p)],
        [0, math.sin(p), math.cos(p)]
    ])

    # Yaw around Z (vertical axis)
    R_yaw = np.array([
        [math.cos(y), -math.sin(y), 0],
        [math.sin(y), math.cos(y), 0],
        [0, 0, 1]
    ])

    return R_yaw @ R_pitch @ R_roll

def project_3d_point(p, cam_pos, cam_rot, fov, w, h):
    # Transform point to camera local space
    p_rel = p - cam_pos
    p_cam = cam_rot.T @ p_rel

    # Camera looks along -Z in standard view, but in our CAD chase:
    # Let's define camera coordinates: X right, Y up, Z forward
    x, y, z = p_cam[0], p_cam[2], p_cam[1]

    if z <= 0.5:
        return None, z  # Behind camera

    aspect = w / h
    f = 1.0 / math.tan(math.radians(fov) / 2.0)
    sx = int(w / 2 + (x * f / z / aspect) * (w / 2))
    sy = int(h / 2 - (y * f / z) * (h / 2))
    return (sx, sy), z

# ============================================================================
# MAIN VIDEO GENERATOR SCRIPT
# ============================================================================

def main():
    width = 1280
    height = 720
    fps = 30
    duration_sec = 15.0
    total_frames = int(duration_sec * fps)  # 450 frames

    out_mp4 = "artifacts/atlascad-supersonic-flight.mp4"
    os.makedirs(os.path.dirname(out_mp4), exist_ok=True)

    print(f"[AtlasCAD Aero Video] Generating {duration_sec}s demonstration at {width}x{height} @ {fps} FPS ({total_frames} frames)...")

    # Load Fonts
    font_mono_bold_path = "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Bold.ttf"
    font_mono_reg_path = "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf"
    font_sans_path = "/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf"

    try:
        f_title = ImageFont.truetype(font_mono_bold_path, 16)
        f_hud = ImageFont.truetype(font_mono_bold_path, 13)
        f_hud_sm = ImageFont.truetype(font_mono_reg_path, 11)
        f_tape_lg = ImageFont.truetype(font_mono_bold_path, 18)
        f_tape_sm = ImageFont.truetype(font_mono_reg_path, 10)
        f_mach = ImageFont.truetype(font_mono_bold_path, 22)
    except Exception:
        f_title = f_hud = f_hud_sm = f_tape_lg = f_tape_sm = f_mach = ImageFont.load_default()

    vertices, faces = get_aircraft_geometry()

    # Launch ffmpeg process streaming raw RGB frames to H.264 MP4
    ffmpeg_cmd = [
        "ffmpeg", "-y",
        "-f", "rawvideo",
        "-vcodec", "rawvideo",
        "-s", f"{width}x{height}",
        "-pix_fmt", "rgb24",
        "-r", str(fps),
        "-i", "-",
        "-c:v", "libx264",
        "-pix_fmt", "yuv420p",
        "-preset", "medium",
        "-crf", "18",
        "-movflags", "+faststart",
        out_mp4
    ]

    proc = subprocess.Popen(ffmpeg_cmd, stdin=subprocess.PIPE)

    # Lighting direction (Sun coming from high right rear)
    light_dir = np.array([0.4, -0.6, 0.7])
    light_dir = light_dir / np.linalg.norm(light_dir)

    for frame_idx in range(total_frames):
        t = frame_idx / fps  # Current time in seconds (0.0 to 15.0)

        # --------------------------------------------------------------------
        # DYNAMIC FLIGHT PHYSICS TRAJECTORY
        # --------------------------------------------------------------------
        # Stage 1 (0-3.5s): Takeoff & Climb Out
        # Stage 2 (3.5-8.0s): High-G Bank Turn & Afterburner Ignition
        # Stage 3 (8.0-11.5s): Supersonic Zoom Climb to Mach 1.55
        # Stage 4 (11.5-15.0s): High Altitude Supersonic Cruise & 360 Orbit Camera

        if t < 3.5:
            # Stage 1: Climb out
            progress = t / 3.5
            pitch = 5.0 + 12.0 * math.sin(progress * math.pi / 2)
            roll = -4.0 * math.sin(progress * math.pi)
            yaw = 45.0 + progress * 8.0
            altitude = 800.0 + progress * 2400.0
            speed_kmh = 380.0 + progress * 420.0
            throttle = 80.0 + progress * 15.0
            afterburner = False
            g_force = 1.15 + 0.3 * math.sin(progress * math.pi)
            aoa = 4.5 + 2.0 * math.sin(progress * math.pi)
            cam_mode = "chase"
            cam_orbit_deg = 0.0
        elif t < 8.0:
            # Stage 2: Afterburner Ignition & High-G Bank Turn
            progress = (t - 3.5) / 4.5
            pitch = 14.0 - 5.0 * math.sin(progress * math.pi)
            roll = -58.0 * math.sin(progress * math.pi)
            yaw = 53.0 + progress * 65.0
            altitude = 3200.0 + progress * 4500.0
            speed_kmh = 800.0 + progress * 780.0
            throttle = 100.0
            afterburner = True
            g_force = 1.4 + 3.2 * math.sin(progress * math.pi)
            aoa = 5.8 + 2.4 * math.sin(progress * math.pi)
            cam_mode = "chase"
            cam_orbit_deg = 0.0
        elif t < 11.5:
            # Stage 3: Supersonic breakout to Mach 1.55
            progress = (t - 8.0) / 3.5
            pitch = 16.0 + 6.0 * math.sin(progress * math.pi / 2)
            roll = -6.0 * (1.0 - progress)
            yaw = 118.0 + progress * 12.0
            altitude = 7700.0 + progress * 4200.0
            speed_kmh = 1580.0 + progress * 450.0
            throttle = 100.0
            afterburner = True
            g_force = 1.6 - 0.4 * progress
            aoa = 3.6 - 0.4 * progress
            cam_mode = "chase"
            cam_orbit_deg = 0.0
        else:
            # Stage 4: High Altitude Cruise & Cinematic Camera Sweep
            progress = (t - 11.5) / 3.5
            pitch = 4.0 + 1.5 * math.sin(progress * math.pi * 2)
            roll = 10.0 * math.sin(progress * math.pi * 2)
            yaw = 130.0 + progress * 10.0
            altitude = 11900.0 + progress * 600.0
            speed_kmh = 2030.0 + 40.0 * math.sin(progress * math.pi)
            throttle = 92.0
            afterburner = True
            g_force = 1.02
            aoa = 3.1
            cam_mode = "orbit"
            cam_orbit_deg = progress * 120.0  # Smooth orbit around aircraft

        # US Standard Atmosphere speed of sound & Mach
        temp_k = 288.15 - 0.0065 * min(altitude, 11000.0) if altitude < 11000.0 else 216.65
        speed_mps = speed_kmh / 3.6
        speed_of_sound = math.sqrt(1.4 * 287.05 * temp_k)
        mach = speed_mps / speed_of_sound
        rho = 1.225 * math.exp(-altitude / 8500.0)
        q_kpa = (0.5 * rho * speed_mps * speed_mps) / 1000.0
        spar_stress = g_force * 115.0
        fos = max(1.0, 880.0 / spar_stress)
        climb_rate = speed_mps * math.sin(math.radians(pitch))

        # --------------------------------------------------------------------
        # RENDER BACKGROUND & SKY HORIZON
        # --------------------------------------------------------------------
        img = Image.new("RGB", (width, height), (13, 21, 39))
        draw = ImageDraw.Draw(img, "RGBA")

        # Dynamic sky gradient based on altitude and roll
        # Upper space darkness fading to stratospheric blue
        horizon_y = int(height / 2 + math.tan(math.radians(pitch)) * 250.0)
        horizon_y = max(100, min(height - 100, horizon_y))

        for y in range(0, height, 4):
            factor = y / height
            if y < horizon_y:
                # Sky: deep navy -> cyan atmospheric glow near horizon
                glow = max(0.0, 1.0 - abs(y - horizon_y) / 400.0)
                r_col = int(8 + glow * 22)
                g_col = int(18 + glow * 55)
                b_col = int(42 + glow * 95)
            else:
                # Earth / terrain haze: dark slate terrain grid
                depth = (y - horizon_y) / (height - horizon_y + 1e-5)
                r_col = int(12 + depth * 15)
                g_col = int(18 + depth * 22)
                b_col = int(28 + depth * 32)
            draw.rectangle([0, y, width, y + 4], fill=(r_col, g_col, b_col, 255))

        # High-speed terrain motion grid lines
        grid_phase = (frame_idx * (speed_kmh / 300.0)) % 40.0
        for gx in range(-width, width * 2, 80):
            draw.line([(gx, horizon_y), (int(width / 2 + (gx - width / 2) * 2.8), height)], fill=(30, 50, 75, 70), width=1)
        for gy in range(horizon_y + 10, height, 35):
            gy_dyn = gy + int(grid_phase * ((gy - horizon_y) / (height - horizon_y)))
            if gy_dyn < height:
                draw.line([(0, gy_dyn), (width, gy_dyn)], fill=(30, 50, 75, 60), width=1)

        # --------------------------------------------------------------------
        # 3D AIRCRAFT TRANSFORMATION & RENDERING
        # --------------------------------------------------------------------
        # Compute aircraft orientation matrix
        R_ac = make_rotation_matrix(roll, pitch, yaw)

        # Camera setup
        if cam_mode == "chase":
            # Chase camera slightly behind and above, responding dynamically to bank
            chase_dist = 28.0
            cam_offset = np.array([
                -chase_dist * math.sin(math.radians(yaw)) - math.sin(math.radians(roll)) * 3.0,
                -chase_dist * math.cos(math.radians(yaw)),
                6.5 + math.sin(math.radians(pitch)) * 2.0
            ])
            cam_pos = cam_offset
            # Camera looks toward aircraft center [0, 0, 0.4]
            look_target = np.array([0.0, 0.0, 0.4])
        else:
            # Orbit camera sweep
            orbit_r = 26.0
            rad = math.radians(cam_orbit_deg - 45.0)
            cam_pos = np.array([
                math.sin(rad) * orbit_r,
                -math.cos(rad) * orbit_r,
                5.0 + math.sin(rad * 2.0) * 2.5
            ])
            look_target = np.array([0.0, 0.0, 0.4])

        # Compute camera rotation matrix to look at target
        cam_fwd = look_target - cam_pos
        cam_fwd = cam_fwd / np.linalg.norm(cam_fwd)
        cam_up = np.array([0.0, 0.0, 1.0])
        cam_right = np.cross(cam_fwd, cam_up)
        cam_right = cam_right / np.linalg.norm(cam_right)
        cam_up = np.cross(cam_right, cam_fwd)
        cam_rot = np.column_stack([cam_right, cam_fwd, cam_up])

        # Transform all vertices into world space
        world_verts = {}
        for key, v in vertices.items():
            world_verts[key] = R_ac @ v

        # --------------------------------------------------------------------
        # SUPERSONIC AFTERBURNER FLAME & SHOCK DIAMONDS
        # --------------------------------------------------------------------
        if afterburner or throttle > 50.0:
            flame_len = 5.5 if afterburner else 2.5
            noz_left_w = world_verts["noz_l"]
            noz_right_w = world_verts["noz_r"]
            back_dir = R_ac @ np.array([0.0, -1.0, 0.0])

            for noz in [noz_left_w, noz_right_w]:
                flame_tip = noz + back_dir * flame_len
                p_noz, _ = project_3d_point(noz, cam_pos, cam_rot, 48.0, width, height)
                p_tip, _ = project_3d_point(flame_tip, cam_pos, cam_rot, 48.0, width, height)

                if p_noz and p_tip:
                    # Outer orange plasma
                    draw.line([p_noz, p_tip], fill=(255, 120, 20, 220), width=8 if afterburner else 4)
                    # Inner white-hot core
                    draw.line([p_noz, p_tip], fill=(255, 230, 180, 255), width=4 if afterburner else 2)

                    # Mach shock diamonds (pulsing cyan diamonds)
                    if afterburner:
                        for d_idx in range(1, 4):
                            d_pos = noz + back_dir * (flame_len * (d_idx / 4.0))
                            p_d, _ = project_3d_point(d_pos, cam_pos, cam_rot, 48.0, width, height)
                            if p_d:
                                d_size = 4 + (frame_idx % 3)
                                draw.polygon([
                                    (p_d[0] - d_size, p_d[1]),
                                    (p_d[0], p_d[1] - d_size),
                                    (p_d[0] + d_size, p_d[1]),
                                    (p_d[0], p_d[1] + d_size)
                                ], fill=(120, 240, 255, 230))

        # --------------------------------------------------------------------
        # TRANSONIC MACH CONE SHOCKWAVE (Visualized when M >= 1.0)
        # --------------------------------------------------------------------
        if mach >= 1.0:
            cone_angle = math.asin(1.0 / mach)
            cone_len = 16.0
            nose_w = world_verts["nose"]
            back_cone = nose_w + (R_ac @ np.array([0.0, -1.0, 0.0])) * cone_len
            cone_radius = cone_len * math.tan(cone_angle)

            # Draw translucent elliptical shock cone rings
            for step in [0.4, 0.7, 1.0]:
                c_center = nose_w + (R_ac @ np.array([0.0, -1.0, 0.0])) * (cone_len * step)
                r_step = cone_radius * step
                p_c, _ = project_3d_point(c_center, cam_pos, cam_rot, 48.0, width, height)
                p_c_r, _ = project_3d_point(c_center + (R_ac @ np.array([r_step, 0.0, 0.0])), cam_pos, cam_rot, 48.0, width, height)
                if p_c and p_c_r:
                    px_r = abs(p_c_r[0] - p_c[0])
                    draw.ellipse([p_c[0] - px_r, p_c[1] - int(px_r * 0.4), p_c[0] + px_r, p_c[1] + int(px_r * 0.4)],
                                 outline=(100, 200, 255, 80), width=1)

        # --------------------------------------------------------------------
        # RENDER 3D AIRCRAFT POLYGON SURFACES (Painter's Algorithm Z-Sort)
        # --------------------------------------------------------------------
        rendered_faces = []
        for face_keys, cat in faces:
            w_pts = [world_verts[k] for k in face_keys]

            # Face centroid & normal for shading
            centroid = np.mean(w_pts, axis=0)
            dist_to_cam = np.linalg.norm(centroid - cam_pos)

            v1 = w_pts[1] - w_pts[0]
            v2 = w_pts[2] - w_pts[0]
            normal = np.cross(v1, v2)
            norm_len = np.linalg.norm(normal)
            if norm_len > 1e-6:
                normal = normal / norm_len
            else:
                normal = np.array([0.0, 0.0, 1.0])

            # Project 2D screen vertices
            screen_pts = []
            valid = True
            for pt in w_pts:
                sp, z = project_3d_point(pt, cam_pos, cam_rot, 48.0, width, height)
                if sp is None or z <= 0.5:
                    valid = False
                    break
                screen_pts.append(sp)

            if valid and len(screen_pts) >= 3:
                # Diffuse Lambertian + Ambient shading
                diffuse = max(0.0, np.dot(normal, light_dir))
                base_col = PALETTE.get(cat, (50, 65, 85))

                if cat == "canopy":
                    # Specular highlight for glass
                    view_dir = (cam_pos - centroid) / np.linalg.norm(cam_pos - centroid)
                    reflect_dir = 2 * np.dot(normal, light_dir) * normal - light_dir
                    spec = max(0.0, np.dot(view_dir, reflect_dir)) ** 12
                    shaded_r = min(255, int(base_col[0] * 0.4 + diffuse * 120 + spec * 180))
                    shaded_g = min(255, int(base_col[1] * 0.5 + diffuse * 140 + spec * 200))
                    shaded_b = min(255, int(base_col[2] * 0.6 + diffuse * 160 + spec * 220))
                else:
                    shaded_r = min(255, int(base_col[0] * (0.35 + 0.65 * diffuse)))
                    shaded_g = min(255, int(base_col[1] * (0.35 + 0.65 * diffuse)))
                    shaded_b = min(255, int(base_col[2] * (0.35 + 0.65 * diffuse)))

                rendered_faces.append((dist_to_cam, screen_pts, (shaded_r, shaded_g, shaded_b, 255)))

        # Sort far-to-near
        rendered_faces.sort(key=lambda x: x[0], reverse=True)

        for _, s_pts, col in rendered_faces:
            draw.polygon(s_pts, fill=col, outline=(56, 189, 248, 120))

        # --------------------------------------------------------------------
        # LIVE TACTICAL HUD OVERLAY (Matches AtlasCAD UI)
        # --------------------------------------------------------------------
        cx = width // 2
        cy = height // 2

        # 1. Pitch Ladder & Artificial Horizon (Rotates with bank angle)
        hud_roll_rad = math.radians(roll)
        pitch_offset = pitch * 4.0

        # Rotate horizon by bank angle
        cos_r = math.cos(hud_roll_rad)
        sin_r = math.sin(hud_roll_rad)

        def rot_hud(x, y):
            # Rotates around center
            xr = x * cos_r - (y + pitch_offset) * sin_r
            yr = x * sin_r + (y + pitch_offset) * cos_r
            return (int(cx + xr), int(cy + yr))

        # Horizon Line
        h_p1 = rot_hud(-180, 0)
        h_p2 = rot_hud(180, 0)
        draw.line([h_p1, h_p2], fill=(34, 211, 238, 200), width=2)

        # Pitch rungs (+20, +10, -10, -20)
        for r_deg in [20, 10, -10, -20]:
            y_r = -r_deg * 4.0
            r_l1 = rot_hud(-70, y_r)
            r_l2 = rot_hud(-30, y_r)
            r_r1 = rot_hud(30, y_r)
            r_r2 = rot_hud(70, y_r)
            draw.line([r_l1, r_l2], fill=(34, 211, 238, 180), width=2)
            draw.line([r_r1, r_r2], fill=(34, 211, 238, 180), width=2)
            # Label
            lbl_pos = rot_hud(-24, y_r)
            draw.text(lbl_pos, f"{r_deg}", fill=(34, 211, 238, 230), font=f_hud_sm)

        # Center Aircraft Boresight Marker (Fixed reticle in green)
        draw.ellipse([cx - 8, cy - 8, cx + 8, cy + 8], outline=(16, 185, 129, 240), width=2)
        draw.line([(cx - 35, cy), (cx - 12, cy)], fill=(16, 185, 129, 240), width=2)
        draw.line([(cx + 12, cy), (cx + 35, cy)], fill=(16, 185, 129, 240), width=2)
        draw.line([(cx, cy - 12), (cx, cy - 8)], fill=(16, 185, 129, 240), width=2)

        # Velocity Vector / Flight Path Marker (Velocity direction)
        fpv_y = int(cy - climb_rate * 1.8)
        draw.ellipse([cx - 7, fpv_y - 7, cx + 7, fpv_y + 7], outline=(56, 189, 248, 240), width=2)
        draw.line([(cx - 18, fpv_y), (cx - 7, fpv_y)], fill=(56, 189, 248, 240), width=2)
        draw.line([(cx + 7, fpv_y), (cx + 18, fpv_y)], fill=(56, 189, 248, 240), width=2)
        draw.line([(cx, fpv_y - 12), (cx, fpv_y - 7)], fill=(56, 189, 248, 240), width=2)

        # Top Heading Tape
        draw.rectangle([cx - 90, 48, cx + 90, 74], fill=(13, 21, 39, 220), outline=(34, 211, 238, 200), width=1)
        hdg_str = f"HDG: {int(yaw) % 360:03d}°"
        draw.text((cx - 45, 52), hdg_str, fill=(255, 255, 255, 255), font=f_hud)

        # Left Airspeed Tape
        kias = int(speed_mps * 1.94384)
        draw.rectangle([35, cy - 85, 145, cy + 85], fill=(13, 21, 39, 220), outline=(34, 211, 238, 200), width=1)
        draw.text((45, cy - 78), "KIAS (kts)", fill=(34, 211, 238, 200), font=f_hud_sm)
        draw.text((45, cy - 50), f"{kias}", fill=(255, 255, 255, 255), font=f_tape_lg)
        draw.text((45, cy - 20), f"{int(speed_kmh)} km/h", fill=(148, 163, 184, 220), font=f_hud_sm)
        draw.text((45, cy + 10), f"q: {q_kpa:.1f} kPa", fill=(56, 189, 248, 220), font=f_hud_sm)
        draw.text((45, cy + 35), f"AoA: {aoa:.1f}°", fill=(16, 185, 129, 220), font=f_hud_sm)
        draw.text((45, cy + 58), f"M {mach:.2f}", fill=(255, 180, 50, 240) if mach >= 1.0 else (34, 211, 238, 220), font=f_hud)

        # Right Altitude Tape
        draw.rectangle([width - 155, cy - 85, width - 45, cy + 85], fill=(13, 21, 39, 220), outline=(34, 211, 238, 200), width=1)
        draw.text((width - 145, cy - 78), "ALT (m)", fill=(34, 211, 238, 200), font=f_hud_sm)
        draw.text((width - 145, cy - 50), f"{int(altitude):,}", fill=(255, 255, 255, 255), font=f_tape_lg)
        draw.text((width - 145, cy - 20), f"FL {int(altitude * 0.0328):03d}", fill=(148, 163, 184, 220), font=f_hud_sm)
        draw.text((width - 145, cy + 10), f"VSI: {climb_rate:+.1f} m/s", fill=(56, 189, 248, 220), font=f_hud_sm)
        draw.text((width - 145, cy + 35), f"P: {int(rho * 287 * temp_k / 100)} hPa", fill=(148, 163, 184, 220), font=f_hud_sm)
        draw.text((width - 145, cy + 58), f"G: {g_force:.2f} G", fill=(239, 68, 68, 240) if g_force > 3.5 else (16, 185, 129, 220), font=f_hud)

        # --------------------------------------------------------------------
        # TOP HEADER & TELEMETRY RIBBON
        # --------------------------------------------------------------------
        # Header banner
        draw.rectangle([0, 0, width, 40], fill=(9, 14, 26, 245))
        draw.line([(0, 40), (width, 40)], fill=(34, 211, 238, 180), width=1)
        draw.text((15, 10), "ATLASCAD AEROSPACE 3D · 6-DOF SUPERSONIC MECHANICAL SIMULATOR", fill=(34, 211, 238, 255), font=f_title)

        # Recording badge & timer
        rec_time_str = f"● REC {int(t // 60):02d}:{int(t % 60):02d}.{int((t * 10) % 10)}"
        draw.rectangle([width - 240, 7, width - 15, 33], fill=(239, 68, 68, 40), outline=(239, 68, 68, 200), width=1)
        draw.text((width - 225, 11), rec_time_str, fill=(239, 68, 68, 255), font=f_hud)

        # Quick stats cards
        stats = [
            ("MACH", f"M {mach:.2f}"),
            ("SPEED", f"{int(speed_kmh):,} km/h"),
            ("ALTITUDE", f"{int(altitude):,} m"),
            ("G-FORCE", f"{g_force:.2f} G"),
            ("CLIMB", f"{climb_rate:+.1f} m/s"),
            ("SPAR STRESS", f"{int(spar_stress)} MPa"),
            ("FoS", f"{fos:.2f}"),
        ]

        card_w = 110
        start_x = 20
        y_card = height - 130
        draw.rectangle([10, y_card - 5, width - 10, height - 10], fill=(9, 14, 26, 235), outline=(30, 41, 59, 200), width=1)

        # Left Propulsion box
        draw.rectangle([20, y_card + 5, 290, height - 20], fill=(15, 23, 42, 220), outline=(51, 65, 85, 200), width=1)
        draw.text((30, y_card + 10), "PROPULSION SYSTEM (GE F110 / SCRAMJET)", fill=(34, 211, 238, 220), font=f_hud_sm)
        thrust_kn = (185 if afterburner else 125) * (throttle / 100.0)
        draw.text((30, y_card + 32), f"THRUST: {thrust_kn:.1f} kN · {throttle:.0f}%", fill=(255, 255, 255, 255), font=f_hud)
        ab_status = "⚡ AFTERBURNER: ENGAGED (MAX THRUST)" if afterburner else "AFTERBURNER: OFF"
        ab_col = (255, 120, 20, 255) if afterburner else (148, 163, 184, 220)
        draw.text((30, y_card + 56), ab_status, fill=ab_col, font=f_hud_sm)
        draw.text((30, y_card + 76), f"Net Fuel Flow: {3.65 if afterburner else 1.42:.2f} kg/s · N1: 99.4%", fill=(148, 163, 184, 200), font=f_hud_sm)

        # Center Aerodynamics Grid
        draw.rectangle([310, y_card + 5, 780, height - 20], fill=(15, 23, 42, 220), outline=(51, 65, 85, 200), width=1)
        draw.text((320, y_card + 10), "REAL-TIME AERODYNAMICS & FEA WING ROOT STRESS DERIVATIVES", fill=(34, 211, 238, 220), font=f_hud_sm)

        cl_val = 0.08 + 4.2 * math.radians(aoa)
        cd_val = 0.018 + (cl_val ** 2) / (math.pi * 0.82 * 3.8) + (0.025 if mach > 0.85 else 0.0)
        lift_kn = q_kpa * 42.0 * cl_val
        drag_kn = q_kpa * 42.0 * cd_val

        col1_x = 320
        col2_x = 550
        draw.text((col1_x, y_card + 32), f"Angle of Attack (α): {aoa:.1f}°", fill=(255, 255, 255, 255), font=f_hud_sm)
        draw.text((col1_x, y_card + 54), f"Lift Coefficient (CL): {cl_val:.3f}", fill=(56, 189, 248, 220), font=f_hud_sm)
        draw.text((col1_x, y_card + 76), f"Drag Coefficient (CD): {cd_val:.3f}", fill=(56, 189, 248, 220), font=f_hud_sm)

        draw.text((col2_x, y_card + 32), f"Lift Force (L): {lift_kn:.1f} kN", fill=(255, 255, 255, 255), font=f_hud_sm)
        draw.text((col2_x, y_card + 54), f"Drag Force (D): {drag_kn:.1f} kN", fill=(148, 163, 184, 220), font=f_hud_sm)
        stress_col = (239, 68, 68, 255) if spar_stress > 400 else (16, 185, 129, 255)
        draw.text((col2_x, y_card + 76), f"Wing Root Spar σvM: {int(spar_stress)} MPa", fill=stress_col, font=f_hud_sm)

        # Right Summary Cards
        draw.rectangle([800, y_card + 5, width - 20, height - 20], fill=(15, 23, 42, 220), outline=(51, 65, 85, 200), width=1)
        draw.text((815, y_card + 10), "STRUCTURAL INTEGRITY & FLIGHT ENVELOPE", fill=(34, 211, 238, 220), font=f_hud_sm)
        draw.text((815, y_card + 32), f"Factor of Safety (FoS): {fos:.2f}", fill=(16, 185, 129, 255) if fos >= 2.0 else (245, 158, 11, 255), font=f_hud)
        draw.text((815, y_card + 56), f"Camera Mode: {cam_mode.upper()} · 6-DOF Active", fill=(148, 163, 184, 220), font=f_hud_sm)
        draw.text((815, y_card + 76), f"CAD Standard: ISO 10303 STEP / WebGL 2.0", fill=(148, 163, 184, 200), font=f_hud_sm)

        # Write frame bytes to ffmpeg stdin
        proc.stdin.write(img.tobytes())

        if (frame_idx + 1) % 60 == 0 or frame_idx == total_frames - 1:
            print(f"  -> Rendered frame {frame_idx + 1}/{total_frames} ({((frame_idx + 1)/total_frames)*100:.1f}%)")

    proc.stdin.close()
    proc.wait()

    if proc.returncode == 0:
        file_size_mb = os.path.getsize(out_mp4) / (1024 * 1024)
        print(f"[AtlasCAD Aero Video] Successfully generated {out_mp4} ({file_size_mb:.2f} MB)")
    else:
        print(f"[AtlasCAD Aero Video] Error generating video, returncode={proc.returncode}")
        sys.exit(1)

if __name__ == "__main__":
    main()
