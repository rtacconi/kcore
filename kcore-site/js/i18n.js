(function () {
  const SUPPORTED = ['en', 'it', 'de', 'zh'];
  const LANG_NAMES = {
    en: 'English',
    it: 'Italiano',
    de: 'Deutsch',
    zh: '中文'
  };
  const LANG_FLAGS = {
    en: '🇬🇧',
    it: '🇮🇹',
    de: '🇩🇪',
    zh: '🇨🇳'
  };
  const DEFAULT = 'en';
  const STORAGE_KEY = 'kcore-lang';

  let translations = {};
  let currentLang = DEFAULT;
  let modal = null;

  function detectLang() {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored && SUPPORTED.includes(stored)) return stored;

    const browser = (navigator.language || '').slice(0, 2).toLowerCase();
    return SUPPORTED.includes(browser) ? browser : DEFAULT;
  }

  function resolveKey(obj, key) {
    return key.split('.').reduce((o, k) => (o && o[k] !== undefined ? o[k] : null), obj);
  }

  function createModal() {
    modal = document.createElement('div');
    modal.className = 'lang-modal-overlay';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-label', 'Select language');
    modal.innerHTML =
      '<div class="lang-modal">' +
        '<div class="lang-modal-header">' +
          '<span class="lang-modal-title">Language</span>' +
          '<button class="lang-modal-close" aria-label="Close">&times;</button>' +
        '</div>' +
        '<ul class="lang-modal-list">' +
          SUPPORTED.map(function (lang) {
            return '<li>' +
              '<button class="lang-modal-item" data-lang="' + lang + '">' +
                '<span class="lang-flag">' + LANG_FLAGS[lang] + '</span>' +
                '<span class="lang-name">' + LANG_NAMES[lang] + '</span>' +
                '<span class="lang-code">' + lang.toUpperCase() + '</span>' +
              '</button>' +
            '</li>';
          }).join('') +
        '</ul>' +
      '</div>';

    modal.addEventListener('click', function (e) {
      if (e.target === modal) closeModal();
    });

    modal.querySelector('.lang-modal-close').addEventListener('click', closeModal);

    modal.querySelectorAll('.lang-modal-item').forEach(function (btn) {
      btn.addEventListener('click', function () {
        switchLang(btn.getAttribute('data-lang'));
        closeModal();
      });
    });

    document.body.appendChild(modal);
  }

  function updateModalActive() {
    if (!modal) return;
    modal.querySelectorAll('.lang-modal-item').forEach(function (btn) {
      btn.classList.toggle('active', btn.getAttribute('data-lang') === currentLang);
    });
  }

  function openModal() {
    if (!modal) createModal();
    updateModalActive();
    modal.classList.add('visible');
    document.body.style.overflow = 'hidden';
  }

  function closeModal() {
    if (!modal) return;
    modal.classList.remove('visible');
    document.body.style.overflow = '';
  }

  function applyTranslations() {
    document.querySelectorAll('[data-i18n]').forEach(function (el) {
      var key = el.getAttribute('data-i18n');
      var val = resolveKey(translations[currentLang], key);
      if (val !== null) el.textContent = val;
    });

    document.querySelectorAll('[data-i18n-html]').forEach(function (el) {
      var key = el.getAttribute('data-i18n-html');
      var val = resolveKey(translations[currentLang], key);
      if (val !== null) el.innerHTML = val;
    });

    document.querySelectorAll('[data-i18n-placeholder]').forEach(function (el) {
      var key = el.getAttribute('data-i18n-placeholder');
      var val = resolveKey(translations[currentLang], key);
      if (val !== null) el.placeholder = val;
    });

    document.documentElement.lang = currentLang;

    document.querySelectorAll('.lang-toggle-btn').forEach(function (btn) {
      btn.innerHTML = '<span class="lang-btn-flag">' + LANG_FLAGS[currentLang] + '</span> ' + currentLang.toUpperCase();
    });

    updateModalActive();
  }

  function switchLang(lang) {
    if (!SUPPORTED.includes(lang)) return;
    currentLang = lang;
    localStorage.setItem(STORAGE_KEY, lang);
    applyTranslations();
  }

  async function loadTranslations() {
    var base = document.querySelector('script[src*="i18n.js"]');
    var prefix = '';
    if (base) {
      var src = base.getAttribute('src');
      prefix = src.replace('js/i18n.js', '');
    }

    var results = await Promise.all(
      SUPPORTED.map(function (lang) {
        return fetch(prefix + 'locales/' + lang + '.json').then(function (r) { return r.json(); });
      })
    );
    SUPPORTED.forEach(function (lang, i) { translations[lang] = results[i]; });
  }

  async function init() {
    currentLang = detectLang();
    await loadTranslations();
    applyTranslations();

    document.querySelectorAll('.lang-toggle-btn').forEach(function (btn) {
      btn.addEventListener('click', openModal);
    });

    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') closeModal();
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

  window.kcoreI18n = { switchLang: switchLang };
})();
