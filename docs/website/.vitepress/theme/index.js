import { h } from 'vue';
import DefaultTheme from 'vitepress/theme';
import Terminal from './Terminal.vue';
import '../../src/parcela.css';

export default {
  extends: DefaultTheme,
  Layout: () => h(DefaultTheme.Layout, null, { 'home-hero-image': () => h(Terminal) }),
};
