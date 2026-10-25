// JavaScript add-in: one STEP file per body, through the host export API.
export async function run(command, params, api) {
  const bodies = api.bodies();
  if (!bodies.length) return { message: 'no bodies to export' };
  for (const b of bodies) {
    const bytes = await api.export('step', [b.id]);
    api.download(bytes, `${b.id}.step`);
  }
  return { message: `exported ${bodies.length} STEP file(s)` };
}
