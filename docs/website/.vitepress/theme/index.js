import { h } from 'vue';
import DefaultTheme from 'vitepress/theme';
import HeroBackground from './HeroBackground.vue';
import HeroDemo from './HeroDemo.vue';
import HeroTitle from './HeroTitle.vue';
import '../../src/parcela.css';

export default {
  extends: DefaultTheme,
  Layout: () => h(DefaultTheme.Layout, null, {
    'home-hero-before': () => h(HeroBackground),
    'home-hero-info': () => h(HeroTitle),
    'home-hero-after': () => h(HeroDemo),
  }),
};
