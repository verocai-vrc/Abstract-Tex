import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { applyRememberedLook } from './lib/look.svelte';

// The text size and colours that were chosen come first, so that the window is not drawn twice.
void applyRememberedLook().then(() => mount(App, { target: document.getElementById('app')! }));
