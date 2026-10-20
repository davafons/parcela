import { h } from 'vue';
import DefaultTheme from 'vitepress/theme';
import HeroBackground from './HeroBackground.vue';
import HeroDemo from './HeroDemo.vue';
import HeroTitle from './HeroTitle.vue';
import HomeCta from './HomeCta.vue';
import SiteFooter from './SiteFooter.vue';
import '../../src/parcela.css';

export default {
  extends: DefaultTheme,
  Layout: () => h(DefaultTheme.Layout, null, {
    'home-hero-before': () => h(HeroBackground),
    'home-hero-info': () => h(HeroTitle),
    'home-hero-after': () => h(HeroDemo),
    // VitePress has no slot after the home content; the section moves itself last with `order`.
    'home-features-after': () => h(HomeCta),
    'layout-bottom': () => h(SiteFooter),
  }),
};
