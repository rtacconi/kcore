(function () {
  const SUPPORTED = ['en', 'it'];
  const DEFAULT = 'en';
  const STORAGE_KEY = 'kcore-lang';

  let translations = {};
  let currentLang = DEFAULT;

  function detectLang() {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored && SUPPORTED.includes(stored)) return stored;

    const browser = (navigator.language || '').slice(0, 2).toLowerCase();
    return SUPPORTED.includes(browser) ? browser : DEFAULT;
  }

  function resolveKey(obj, key) {
    return key.split('.').reduce((o, k) => (o && o[k] !== undefined ? o[k] : null), obj);
  }

  function applyTranslations() {
    document.querySelectorAll('[data-i18n]').forEach(el => {
      const key = el.getAttribute('data-i18n');
      const val = resolveKey(translations[currentLang], key);
      if (val !== null) el.textContent = val;
    });

    document.querySelectorAll('[data-i18n-html]').forEach(el => {
      const key = el.getAttribute('data-i18n-html');
      const val = resolveKey(translations[currentLang], key);
      if (val !== null) el.innerHTML = val;
    });

    document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
      const key = el.getAttribute('data-i18n-placeholder');
      const val = resolveKey(translations[currentLang], key);
      if (val !== null) el.placeholder = val;
    });

    document.documentElement.lang = currentLang;

    document.querySelectorAll('.lang-toggle-btn').forEach(btn => {
      btn.textContent = currentLang === 'en' ? 'IT' : 'EN';
      btn.setAttribute('aria-label',
        currentLang === 'en' ? 'Passa all\'italiano' : 'Switch to English');
    });
  }

  function switchLang(lang) {
    if (!SUPPORTED.includes(lang)) return;
    currentLang = lang;
    localStorage.setItem(STORAGE_KEY, lang);
    applyTranslations();
  }

  function toggleLang() {
    switchLang(currentLang === 'en' ? 'it' : 'en');
  }

  async function loadTranslations() {
    const base = document.querySelector('script[src*="i18n.js"]');
    let prefix = '';
    if (base) {
      const src = base.getAttribute('src');
      prefix = src.replace('js/i18n.js', '');
    }

    const [en, it] = await Promise.all([
      fetch(prefix + 'locales/en.json').then(r => r.json()),
      fetch(prefix + 'locales/it.json').then(r => r.json())
    ]);
    translations = { en, it };
  }

  async function init() {
    currentLang = detectLang();
    await loadTranslations();
    applyTranslations();

    document.querySelectorAll('.lang-toggle-btn').forEach(btn => {
      btn.addEventListener('click', toggleLang);
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

  window.kcoreI18n = { switchLang, toggleLang };
})();
