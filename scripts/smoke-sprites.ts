// Sprite live-fire verification. Run with a real OPENAI_API_KEY: `bun scripts/smoke-sprites.ts`.
// Synthesizes a dummy cat photo + profile, cheaply probes the OpenAI image model
// (ensureBackground = 1 call), and on rejection lists OpenAI image-capable models.
// If the probe passes, runs generateAllSprites() and verifies cat 7 emotion PNGs
// have alpha + the UI assets (icons, logo) were written to public/assets.
import { mkdir, writeFile } from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';
import { loadEnv } from '../server/src/env.ts';
import { OPENAI_IMAGE_MODEL } from '../server/src/llm/models.ts';
import { assetsPath, dataPath } from '../server/src/paths.ts';
import { ensureBackground, generateAllSprites } from '../server/src/art/spritegen.ts';
import { alphaStats } from '../server/src/art/keying.ts';
import type { Emotion, Profile } from '../src/contracts.ts';

const EMOTIONS: Emotion[] = ['neutral', 'surprised', 'delighted', 'shy', 'content', 'angry', 'sad'];
const PUBLIC_ASSETS = fileURLToPath(new URL('../public/assets', import.meta.url));

const CAT_SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512">
  <rect width="512" height="512" fill="#dfe7ee"/>
  <polygon points="150,150 120,60 220,120" fill="#e6a24a" stroke="#8a5a1e" stroke-width="6"/>
  <polygon points="362,150 392,60 292,120" fill="#e6a24a" stroke="#8a5a1e" stroke-width="6"/>
  <ellipse cx="256" cy="290" rx="160" ry="150" fill="#eaa94e" stroke="#8a5a1e" stroke-width="6"/>
  <ellipse cx="200" cy="270" rx="26" ry="32" fill="#3a3a3a"/>
  <ellipse cx="312" cy="270" rx="26" ry="32" fill="#3a3a3a"/>
  <polygon points="256,315 240,335 272,335" fill="#d46a7a"/>
  <line x1="110" y1="320" x2="200" y2="330" stroke="#6b4f3a" stroke-width="4"/>
  <line x1="402" y1="320" x2="312" y2="330" stroke="#6b4f3a" stroke-width="4"/>
</svg>`;

async function listOpenAiImageModels(): Promise<void> {
  const key = process.env.OPENAI_API_KEY;
  if (!key) {
    console.error('[smoke] no OPENAI_API_KEY to list models');
    return;
  }
  const res = await fetch('https://api.openai.com/v1/models', { headers: { Authorization: `Bearer ${key}` } });
  if (!res.ok) {
    console.error(`[smoke] models.list failed: ${res.status} ${await res.text()}`);
    return;
  }
  const body = (await res.json()) as { data?: { id: string }[] };
  const imageModels = (body.data ?? []).map((m) => m.id).filter((id) => /image/i.test(id)).sort();
  console.error('[smoke] OpenAI image-capable model ids:\n' + (imageModels.join('\n') || '(none found)'));
}

async function main(): Promise<void> {
  const env = loadEnv();
  console.log(`[smoke] key source: ${env.source}; image model: ${OPENAI_IMAGE_MODEL}; quality: ${process.env.OPENAI_IMAGE_QUALITY ?? 'low'}`);

  await mkdir(assetsPath(), { recursive: true });
  const catPhotoPath = assetsPath('cat-photo.png');
  if (!existsSync(catPhotoPath)) {
    await sharp(Buffer.from(CAT_SVG)).png().toFile(catPhotoPath);
    console.log(`[smoke] wrote dummy cat photo → ${catPhotoPath}`);
  }
  const profile: Profile = {
    user: { name: '아리', persona: '호기심 많고 다정한 20대, 동그란 안경', likes: ['별', '코코아'] },
    cat: { name: '치즈', breed: '코리안 숏헤어 치즈태비', personality: '겁많지만 호기심 대장', quirks: ['꾹꾹이', '상자사랑'] },
    wishes: ['밤바다에서 낚시하기'],
  };
  await writeFile(dataPath('profile.json'), JSON.stringify(profile, null, 2));

  // cheap single-call probe (1 images.generate)
  try {
    const url = await ensureBackground('_smoke_probe', 'a cozy paper night harbor with a little dock');
    console.log(`[smoke] image-model probe OK → ${url}`);
  } catch (err) {
    console.error(`[smoke] image-model probe FAILED: ${err instanceof Error ? err.message : String(err)}`);
    await listOpenAiImageModels();
    throw err;
  }

  console.log('[smoke] running generateAllSprites() …');
  const t0 = Date.now();
  const manifest = await generateAllSprites();
  console.log(`[smoke] generateAllSprites done in ${((Date.now() - t0) / 1000).toFixed(0)}s`);

  console.log('[smoke] cat portrait alpha check:');
  let ok = 0;
  for (const em of EMOTIONS) {
    const stats = await alphaStats(readFileSync(assetsPath('sprites', 'cat', `${em}.png`)));
    const pass = stats.hasAlpha && stats.transparentPct >= 12;
    if (pass) ok++;
    console.log(`  ${pass ? '✓' : '✗'} ${em.padEnd(9)} ${stats.width}×${stats.height} alpha=${stats.hasAlpha} transparent=${stats.transparentPct}%`);
  }
  console.log(`[smoke] cat emotions passing (alpha + ≥12% transparent): ${ok}/7`);
  console.log(`[smoke] me emotions present: ${EMOTIONS.filter((e) => manifest.portraits.me[e]).length}/7`);

  console.log('[smoke] UI assets:');
  for (const f of ['icon-192.png', 'icon-512.png', 'logo.png']) {
    const p = join(PUBLIC_ASSETS, f);
    if (existsSync(p)) {
      const meta = await sharp(p).metadata();
      console.log(`  ✓ ${f} ${meta.width}×${meta.height} alpha=${meta.hasAlpha}`);
    } else {
      console.log(`  ✗ ${f} MISSING`);
    }
  }
  console.log(`[smoke] manifest.ui = ${JSON.stringify(manifest.ui)}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
