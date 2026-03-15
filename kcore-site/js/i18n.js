(function () {
  var SUPPORTED = ['en', 'es', 'it', 'de', 'zh'];
  var LANG_NAMES = {
    en: 'English',
    es: 'Español',
    it: 'Italiano',
    de: 'Deutsch',
    zh: '中文'
  };
  var LANG_FLAGS = {
    en: '🇬🇧',
    es: '🇪🇸',
    it: '🇮🇹',
    de: '🇩🇪',
    zh: '🇨🇳'
  };
  var DEFAULT = 'en';
  var STORAGE_KEY = 'kcore-lang';

  var translations = {};
  var currentLang = DEFAULT;
  var modal = null;
  var loaded = false;

  function detectLang() {
    try {
      var stored = localStorage.getItem(STORAGE_KEY);
      if (stored && SUPPORTED.indexOf(stored) !== -1) return stored;
    } catch (e) {}

    var browser = (navigator.language || '').slice(0, 2).toLowerCase();
    return SUPPORTED.indexOf(browser) !== -1 ? browser : DEFAULT;
  }

  function resolveKey(obj, key) {
    if (!obj) return null;
    var parts = key.split('.');
    var cur = obj;
    for (var i = 0; i < parts.length; i++) {
      if (cur[parts[i]] === undefined) return null;
      cur = cur[parts[i]];
    }
    return cur;
  }

  function createModal() {
    modal = document.createElement('div');
    modal.className = 'lang-modal-overlay';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-label', 'Select language');

    var items = '';
    for (var i = 0; i < SUPPORTED.length; i++) {
      var lang = SUPPORTED[i];
      items += '<li>' +
        '<button class="lang-modal-item" data-lang="' + lang + '">' +
          '<span class="lang-flag">' + LANG_FLAGS[lang] + '</span>' +
          '<span class="lang-name">' + LANG_NAMES[lang] + '</span>' +
          '<span class="lang-code">' + lang.toUpperCase() + '</span>' +
        '</button>' +
      '</li>';
    }

    modal.innerHTML =
      '<div class="lang-modal">' +
        '<div class="lang-modal-header">' +
          '<span class="lang-modal-title">Language</span>' +
          '<button class="lang-modal-close" aria-label="Close">&times;</button>' +
        '</div>' +
        '<ul class="lang-modal-list">' + items + '</ul>' +
      '</div>';

    modal.addEventListener('click', function (e) {
      if (e.target === modal) closeModal();
    });

    modal.querySelector('.lang-modal-close').addEventListener('click', function (e) {
      e.preventDefault();
      e.stopPropagation();
      closeModal();
    });

    var langBtns = modal.querySelectorAll('.lang-modal-item');
    for (var j = 0; j < langBtns.length; j++) {
      (function (btn) {
        btn.addEventListener('click', function (e) {
          e.preventDefault();
          e.stopPropagation();
          switchLang(btn.getAttribute('data-lang'));
          closeModal();
        });
      })(langBtns[j]);
    }

    document.body.appendChild(modal);
  }

  function updateModalActive() {
    if (!modal) return;
    var btns = modal.querySelectorAll('.lang-modal-item');
    for (var i = 0; i < btns.length; i++) {
      if (btns[i].getAttribute('data-lang') === currentLang) {
        btns[i].classList.add('active');
      } else {
        btns[i].classList.remove('active');
      }
    }
  }

  function openModal(e) {
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }
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

  function updateButton() {
    var btns = document.querySelectorAll('.lang-toggle-btn');
    for (var i = 0; i < btns.length; i++) {
      btns[i].innerHTML = '<span class="lang-btn-flag">' + LANG_FLAGS[currentLang] + '</span> ' + currentLang.toUpperCase();
    }
  }

  function applyTranslations() {
    if (!loaded) return;

    var els = document.querySelectorAll('[data-i18n]');
    for (var i = 0; i < els.length; i++) {
      var key = els[i].getAttribute('data-i18n');
      var val = resolveKey(translations[currentLang], key);
      if (val !== null) els[i].textContent = val;
    }

    var htmlEls = document.querySelectorAll('[data-i18n-html]');
    for (var j = 0; j < htmlEls.length; j++) {
      var hkey = htmlEls[j].getAttribute('data-i18n-html');
      var hval = resolveKey(translations[currentLang], hkey);
      if (hval !== null) htmlEls[j].innerHTML = hval;
    }

    var phEls = document.querySelectorAll('[data-i18n-placeholder]');
    for (var k = 0; k < phEls.length; k++) {
      var pkey = phEls[k].getAttribute('data-i18n-placeholder');
      var pval = resolveKey(translations[currentLang], pkey);
      if (pval !== null) phEls[k].placeholder = pval;
    }

    document.documentElement.lang = currentLang;
    updateButton();
    updateModalActive();
  }

  function switchLang(lang) {
    if (SUPPORTED.indexOf(lang) === -1) return;
    currentLang = lang;
    try { localStorage.setItem(STORAGE_KEY, lang); } catch (e) {}
    applyTranslations();
  }

  function getPrefix() {
    var base = document.querySelector('script[src*="i18n.js"]');
    if (base) {
      var src = base.getAttribute('src');
      var idx = src.indexOf('js/i18n.js');
      if (idx !== -1) return src.substring(0, idx);
    }
    return '';
  }

  function loadTranslations() {
    var prefix = getPrefix();
    var pending = SUPPORTED.length;
    var failed = false;

    for (var i = 0; i < SUPPORTED.length; i++) {
      (function (lang) {
        var url = prefix + 'locales/' + lang + '.json';
        var xhr = new XMLHttpRequest();
        xhr.open('GET', url, true);
        xhr.onreadystatechange = function () {
          if (xhr.readyState !== 4) return;
          if (xhr.status === 200) {
            try {
              translations[lang] = JSON.parse(xhr.responseText);
            } catch (e) {
              console.warn('i18n: failed to parse ' + url);
            }
          } else {
            console.warn('i18n: failed to load ' + url + ' (' + xhr.status + ')');
          }
          pending--;
          if (pending === 0) {
            loaded = true;
            applyTranslations();
          }
        };
        xhr.send();
      })(SUPPORTED[i]);
    }
  }

  function bindButtons() {
    var btns = document.querySelectorAll('.lang-toggle-btn');
    for (var i = 0; i < btns.length; i++) {
      btns[i].addEventListener('click', openModal);
    }
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') closeModal();
    });
  }

  function init() {
    currentLang = detectLang();
    updateButton();
    bindButtons();
    loadTranslations();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

  window.kcoreI18n = { switchLang: switchLang };
})();
