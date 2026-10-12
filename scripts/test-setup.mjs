// happy-dom now implements Web Animations. Component tests inspect settled UI;
// motion tests clear storage and explicitly exercise the enabled preference.
if (typeof localStorage !== 'undefined') {
  localStorage.setItem('lovelycarve-ui-animations', '"off"');
}
