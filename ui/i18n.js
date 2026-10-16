// UI language. Portuguese (pt-BR) is the default; English keys are the source strings.
// A MutationObserver translates text/title/placeholder as the UI renders, so every panel,
// menu, modal and toast is covered without sprinkling t() calls around the code.

export const lang = (() => { try { return localStorage.getItem('lang') || 'pt'; } catch { return 'pt'; } })();
export function setLang(l) {
  try { localStorage.setItem('lang', l); } catch {}
  location.reload();
}

const PT = {
  // top bar / tabs
  '‹ Projects': '‹ Projetos', 'Back to all projects': 'Voltar a todos os projetos', 'Project name': 'Nome do projeto',
  'Undo (Ctrl+Z)': 'Desfazer (Ctrl+Z)', 'Redo (Ctrl+Shift+Z)': 'Refazer (Ctrl+Shift+Z)',
  'Save as…': 'Salvar como…', 'Save a copy of the project to a file': 'Salvar uma cópia do projeto em um arquivo',
  'Connect Claude': 'Conectar Claude', 'Claude connected': 'Claude conectado', 'Connect the Claude app to this editor': 'Conectar o app do Claude a este editor',
  'Saved automatically': 'Salvo automaticamente',
  'Media': 'Mídia', 'Script': 'Roteiro', 'Clip': 'Clipe', 'Export': 'Exportar',
  // media pane
  '+ Import media': '+ Importar mídia', 'or drop files anywhere': 'ou solte arquivos em qualquer lugar',
  'preparing…': 'preparando…', 'Add to timeline': 'Adicionar à timeline', 'Add video only (no sound)': 'Adicionar só o vídeo (sem som)',
  'Add audio only': 'Adicionar só o áudio', 'Remove from project': 'Remover do projeto', 'Remove media?': 'Remover mídia?', 'Remove': 'Remover',
  // script pane
  'paste what you intend to say. Claude uses it to fix transcription mistakes.': 'cole o que você pretende falar. O Claude usa isso para corrigir erros da transcrição.',
  'Paste your script here…': 'Cole seu roteiro aqui…', 'Transcript': 'Transcrição', 'Spoken language': 'Idioma falado', 'Auto': 'Auto', 'Transcribe': 'Transcrever',
  'No transcript yet. Transcribe a clip with Whisper, or ask Claude to.': 'Ainda sem transcrição. Transcreva um clipe com o Whisper ou peça ao Claude.',
  // claude pane
  'Claude connection': 'Conexão com o Claude',
  'Cutlight runs a local MCP server while it is open. Claude can do everything you can, see the screen and drive the UI.': 'O Cutlight roda um servidor MCP local enquanto está aberto. O Claude pode fazer tudo o que você faz, ver a tela e controlar a interface.',
  'Connect to the Claude app': 'Conectar ao app do Claude', 'Reconnect to the Claude app': 'Reconectar ao app do Claude', 'Manual setup: Claude Code (terminal)': 'Configuração manual: Claude Code (terminal)', 'Manual setup: Claude app': 'Configuração manual: app do Claude', '(claude_desktop_config.json)': '(claude_desktop_config.json)',
  'Click to copy': 'Clique para copiar',
  'Try asking': 'Experimente pedir',
  '“Import these clips and cut all the silences.”': '“Importe estes clipes e corte todos os silêncios.”',
  '“Compare my script with the transcript and fix errors.”': '“Compare meu roteiro com a transcrição e corrija os erros.”',
  '“Add an animated lower third with my name at 2s.”': '“Adicione um lower third animado com meu nome aos 2s.”',
  '“Make it a Reel, normalize the audio and export.”': '“Deixe no formato Reel, normalize o áudio e exporte.”',
  'Connecting…': 'Conectando…', 'Connected.': 'Conectado.',
  'Connected. Fully quit and reopen the Claude app to load the Cutlight tools.': 'Conectado. Feche totalmente o app do Claude (inclusive na bandeja) e abra de novo para carregar as ferramentas do Cutlight.',
  'Connect Cutlight to Claude': 'Conectar o Cutlight ao Claude',
  'Let Claude edit with you: cut, animate, add graphics and render. One click links this editor to the Claude app.': 'Deixe o Claude editar com você: cortar, animar, adicionar gráficos e renderizar. Um clique liga este editor ao app do Claude.',
  'Not now': 'Agora não', 'Connect to Claude': 'Conectar ao Claude',
  // transport / timeline
  'Go to start (Home)': 'Ir ao início (Home)', 'Back 1 frame (←)': 'Voltar 1 quadro (←)', 'Play / pause (Space)': 'Tocar / pausar (Espaço)', 'Forward 1 frame (→)': 'Avançar 1 quadro (→)',
  'Split': 'Dividir', 'Delete': 'Apagar', 'Ripple delete': 'Apagar e fechar lacuna',
  'Split at playhead (S)': 'Dividir no cursor (S)', 'Delete (Del)': 'Apagar (Del)', 'Delete and close the gap (Shift+Del)': 'Apagar e fechar a lacuna (Shift+Del)',
  '+ Video track': '+ Trilha de vídeo', '+ Audio track': '+ Trilha de áudio', 'Organize ▾': 'Organizar ▾', 'Snap': 'Ímã',
  'Zoom out (-)': 'Diminuir zoom (-)', 'Zoom in (+)': 'Aumentar zoom (+)', 'Fit': 'Ajustar',
  'Close all gaps': 'Fechar todas as lacunas', 'Tidy (gaps + empty tracks)': 'Arrumar (lacunas + trilhas vazias)', 'Move everything to start': 'Mover tudo para o início',
  'Split at playhead': 'Dividir no cursor', 'Mute': 'Silenciar', 'Hide': 'Ocultar', 'Lock': 'Bloquear', 'Remove track': 'Remover trilha',
  'Double-click to rename': 'Clique duplo para renomear', 'Track name': 'Nome da trilha', 'Remove track?': 'Remover trilha?',
  'no audio': 'sem áudio',
  // inspector
  'Select a clip to edit it': 'Selecione um clipe para editar', 'Space play': 'Espaço: tocar', 'S split': 'S: dividir', 'Del delete': 'Del: apagar',
  'video': 'vídeo', 'audio': 'áudio', 'image': 'imagem', 'html': 'html',
  'Speed': 'Velocidade', 'Volume': 'Volume', 'Fade in': 'Fade in', 'Fade out': 'Fade out', 'Framing': 'Enquadramento', 'Fill': 'Preencher',
  'Opacity': 'Opacidade', 'Scale': 'Escala', 'Position X': 'Posição X', 'Position Y': 'Posição Y', 'Transition': 'Transição', 'Length': 'Duração',
  'None': 'Nenhuma', 'Dissolve': 'Dissolver', 'Dip to black': 'Passar pelo preto', 'Slide ←': 'Deslizar ←', 'Slide →': 'Deslizar →', 'Slide ↑': 'Deslizar ↑', 'Slide ↓': 'Deslizar ↓',
  'Detach audio': 'Separar áudio', 'Delete audio': 'Apagar áudio', 'Restore audio': 'Restaurar áudio', 'Delete video (keep audio)': 'Apagar vídeo (manter áudio)',
  'Edit HTML': 'Editar HTML', 'Delete clip': 'Apagar clipe', 'HTML animation': 'Animação HTML', 'Save': 'Salvar',
  'Self-contained, transparent background. Animate with CSS or define': 'Autocontido, fundo transparente. Anime com CSS ou defina',
  'Claude can write this for you.': 'O Claude pode escrever isso para você.',
  // export
  'Format': 'Formato', 'Quality': 'Qualidade', 'Draft': 'Rascunho', 'Good': 'Boa', 'High': 'Alta', 'Max': 'Máxima',
  'Draft renders from the proxies, fastest. Max is smallest-loss, slowest.': 'Rascunho renderiza a partir dos proxies, o mais rápido. Máxima tem a menor perda, e é a mais lenta.',
  'Video kbps': 'Vídeo kbps', '0 = auto': '0 = auto', 'Audio': 'Áudio', 'Bitrate': 'Taxa de bits', 'Sample rate': 'Taxa de amostragem', 'Channels': 'Canais',
  'Stereo': 'Estéreo', 'Mono': 'Mono', 'Normalize loudness (−14 LUFS)': 'Normalizar volume (−14 LUFS)',
  'Use GPU encoder when available': 'Usar codificador da GPU quando disponível', 'Container': 'Contêiner', 'Export video': 'Exportar vídeo', 'Custom': 'Personalizado',
  'Add something to the timeline first': 'Adicione algo à timeline primeiro', 'Video': 'Vídeo',
  'Feed portrait': 'Feed retrato', 'Feed square': 'Feed quadrado', 'Feed landscape': 'Feed paisagem',
  '9:16 · up to 3 min': '9:16 · até 3 min', '9:16 · 60 s per card': '9:16 · 60 s por card',
  // render modal
  'Rendering…': 'Renderizando…', 'Export finished': 'Exportação concluída', 'Cancelled': 'Cancelado', 'Export failed': 'Falha na exportação',
  'Cancel': 'Cancelar', 'Close': 'Fechar', 'Preparing…': 'Preparando…', 'Finished': 'Concluído', 'OK': 'OK',
  // start screen
  'Your projects': 'Seus projetos', 'Open file…': 'Abrir arquivo…', 'New project': 'Novo projeto', 'Start from an empty timeline': 'Começar com uma timeline vazia',
  'Remove from list': 'Remover da lista', 'Name your project': 'Dê um nome ao projeto', 'Remove project?': 'Remover projeto?', 'just now': 'agora há pouco',
  // toasts
  'Audio files go on audio tracks': 'Arquivos de áudio vão em trilhas de áudio', 'Copied': 'Copiado', 'Projects save automatically': 'Os projetos são salvos automaticamente',
  'Saved': 'Salvo', 'Transcribing…': 'Transcrevendo…', 'Transcribing with Whisper…': 'Transcrevendo com o Whisper…', 'Import media with audio first': 'Importe uma mídia com áudio primeiro',
};

Object.assign(PT, {
  'Modern': 'Moderno', 'Clean Healthy': 'Clean Healthy', 'Liquid': 'Liquid',
  'Precise tech look: graphite, neon edges, isometric layers and glow.': 'Visual tech preciso: grafite, bordas neon, camadas isométricas e brilho.',
  'Fresh and calm: soft greens, clay-like 3D objects, gentle floating motion.': 'Fresco e calmo: verdes suaves, objetos 3D estilo argila, movimento flutuante delicado.',
  'Fluid and glossy: glass, gel blobs, metaball morphs and elastic motion.': 'Fluido e brilhante: vidro, gotas de gel, morphs de metaballs e movimento elástico.',
  'Brief': 'Briefing', 'What should the video be about? Audience, platform, length, call to action. Needed to create a video from scratch.': 'Sobre o que é o vídeo? Público, plataforma, duração, chamada para ação. Necessário para criar um vídeo do zero.',
  'e.g. 45 s Reel explaining how X works, for beginners…': 'ex.: Reel de 45 s explicando como X funciona, para iniciantes…',
  'Visual style': 'Estilo visual', 'The look: colours, materials, 3D and motion. Claude decides composition and positioning.': 'O visual: cores, materiais, 3D e movimento. O Claude decide a composição e o posicionamento.',
  'Style': 'Estilo', 'Notes': 'Notas', 'Style and tone': 'Estilo e tom',
  'Claude follows this for pacing, animations, drawings, captions and sound.': 'O Claude segue isso no ritmo, animações, desenhos, legendas e som.',
  'Clean & Minimal': 'Limpo e minimalista', 'Bold & Energetic': 'Ousado e enérgico', 'Playful & Illustrated': 'Divertido e ilustrado',
  'Calm, premium, editorial. Few things on screen, elegant motion.': 'Calmo, premium, editorial. Poucos elementos na tela, movimento elegante.',
  'Creator-style: big kinetic type, fast cuts, punchy motion.': 'Estilo creator: tipografia grande em movimento, cortes rápidos, animações marcantes.',
  'Friendly, warm, hand-drawn feel with bouncy motion.': 'Amigável, acolhedor, com traço de desenho à mão e movimento elástico.',
  'Design system (DESIGN.md)': 'Design system (DESIGN.md)',
  'Colours, fonts and shapes. Paste one from styles.refero.design; Claude uses only these.': 'Cores, fontes e formas. Cole um de styles.refero.design; o Claude usa somente isso.',
  'Paste DESIGN.md here…': 'Cole o DESIGN.md aqui…',
  'Pin a note on a moment of the video. Claude fixes exactly what you list.': 'Fixe uma nota em um momento do vídeo. O Claude corrige exatamente o que você listar.',
  '+ Add note at playhead': '+ Adicionar nota no cursor', 'Copy': 'Copiar', 'Copy all open notes as one message for Claude': 'Copiar todas as notas abertas como uma mensagem para o Claude',
  'No notes yet. Watch the video, pause where something is wrong and add a note.': 'Ainda sem notas. Assista ao vídeo, pause onde algo estiver errado e adicione uma nota.',
  '+ Note': '+ Nota', 'Pin a note here (N)': 'Fixar uma nota aqui (N)', 'Done': 'Feito', 'Delete note': 'Apagar nota',
  'No open notes': 'Nenhuma nota aberta', 'Notes copied. Paste them to Claude': 'Notas copiadas. Cole no Claude',
  'Pixabay': 'Pixabay', 'Lets Claude find free photos, illustrations and vectors. Paste your free API key here (pixabay.com/api/docs), never in chat.': 'Permite ao Claude achar fotos, ilustrações e vetores gratuitos. Cole sua chave de API gratuita aqui (pixabay.com/api/docs), nunca no chat.',
  'Pixabay API key': 'Chave de API do Pixabay', 'Save': 'Salvar', 'Key saved. Claude can search Pixabay.': 'Chave salva. O Claude pode buscar no Pixabay.', 'No key yet.': 'Ainda sem chave.', 'Pixabay key saved': 'Chave do Pixabay salva',
  'Editing rules Claude learned': 'Regras de edição que o Claude aprendeu', 'Every correction you give becomes a rule, so mistakes are not repeated.': 'Cada correção sua vira uma regra, para os erros não se repetirem.',
  'No rules yet.': 'Ainda sem regras.', 'Delete rule': 'Apagar regra',
});

const plural = (n, a, b) => (Number(n) === 1 ? a : b);
const PATTERNS = [
  [/^(\d+) clips?$/, (m) => `${m[1]} ${plural(m[1], 'clipe', 'clipes')}`],
  [/^(\d+) min ago$/, (m) => `há ${m[1]} min`],
  [/^(\d+) h ago$/, (m) => `há ${m[1]} h`],
  [/^(\d+) days ago$/, (m) => `há ${m[1]} dias`],
  [/^up to (\d+) min$/, (m) => `até ${m[1]} min`],
  [/^Transcribed (\d+) segments$/, (m) => `${m[1]} segmentos transcritos`],
  [/^Encoding \((.+)\)$/, (m) => `Codificando (${m[1]})`],
  [/^Rendering animation (\d+)\/(\d+)…$/, (m) => `Renderizando animação ${m[1]}/${m[2]}…`],
  [/^(.+) unavailable, retrying…$/, (m) => `${m[1]} indisponível, tentando de novo…`],
  [/^Longer than (\d+)s limit for (.+)\.$/, (m) => `Mais longo que o limite de ${m[1]}s para ${m[2]}.`],
  [/^“(.+)” and every clip using it will be removed\.$/, (m) => `“${m[1]}” e todos os clipes que o usam serão removidos.`],
  [/^“(.+)” will be removed from this list( and its project file deleted \(your media files are not touched\))?\.$/, (m) =>
    `“${m[1]}” será removido desta lista${m[2] ? ' e o arquivo do projeto será apagado (seus arquivos de mídia não são tocados)' : ''}.`],
  [/^(.+) and its (\d+) clip\(s\) will be removed\.$/, (m) => `${m[1]} e seus ${m[2]} clipe(s) serão removidos.`],
];

const SKIP = 'textarea,input,[contenteditable],.nm,.pn:not(.pt),.lab,code,script,style';

function lookupOne(t) {
  if (Object.prototype.hasOwnProperty.call(PT, t)) return PT[t];
  for (const [re, fn] of PATTERNS) {
    const m = t.match(re);
    if (m) return fn(m);
  }
  return undefined;
}
function lookup(t) {
  const r = lookupOne(t);
  if (r !== undefined) return r;
  if (t.includes(' · ')) {
    let changed = false;
    const out = t.split(' · ').map((p) => { const x = lookupOne(p.trim()); if (x !== undefined) { changed = true; return x; } return p; });
    if (changed) return out.join(' · ');
  }
  return undefined;
}
function tx(s) {
  const t = s.trim();
  if (!t) return s;
  const r = lookup(t);
  return r === undefined ? s : s.replace(t, r);
}

function fixText(n) {
  if (n.parentElement && n.parentElement.closest(SKIP)) return;
  const v = tx(n.nodeValue);
  if (v !== n.nodeValue) n.nodeValue = v;
}
function fixAttrs(el) {
  for (const a of ['title', 'placeholder', 'label']) {
    const v = el.getAttribute && el.getAttribute(a);
    if (v) { const r = tx(v); if (r !== v) el.setAttribute(a, r); }
  }
}
function walk(node) {
  if (node.nodeType === 3) return fixText(node);
  if (node.nodeType !== 1) return;
  if (node.matches && node.matches('script,style')) return;
  fixAttrs(node);
  node.querySelectorAll && node.querySelectorAll('[title],[placeholder],[label]').forEach(fixAttrs);
  const tw = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
  let n;
  while ((n = tw.nextNode())) fixText(n);
}

if (lang === 'pt') {
  document.documentElement.lang = 'pt-BR';
  walk(document.body);
  new MutationObserver((muts) => {
    for (const m of muts) {
      if (m.type === 'childList') m.addedNodes.forEach(walk);
      else if (m.type === 'characterData') fixText(m.target);
      else if (m.type === 'attributes') fixAttrs(m.target);
    }
  }).observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ['title', 'placeholder', 'label'] });
}
