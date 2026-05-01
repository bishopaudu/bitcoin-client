import { invoke, transformCallback } from "./@tauri-apps_api_tauri.js";
//#region node_modules/@tauri-apps/api/helpers/tauri.js
/** @ignore */
async function invokeTauriCommand(command) {
	return invoke("tauri", command);
}
//#endregion
//#region node_modules/@tauri-apps/api/helpers/event.js
/**
* Unregister the event listener associated with the given name and id.
*
* @ignore
* @param event The event name
* @param eventId Event identifier
* @returns
*/
async function _unlisten(event, eventId) {
	return invokeTauriCommand({
		__tauriModule: "Event",
		message: {
			cmd: "unlisten",
			event,
			eventId
		}
	});
}
/**
* Emits an event to the backend.
*
* @param event Event name. Must include only alphanumeric characters, `-`, `/`, `:` and `_`.
* @param [windowLabel] The label of the window to which the event is sent, if null/undefined the event will be sent to all windows
* @param [payload] Event payload
* @returns
*/
async function emit$1(event, windowLabel, payload) {
	await invokeTauriCommand({
		__tauriModule: "Event",
		message: {
			cmd: "emit",
			event,
			windowLabel,
			payload
		}
	});
}
/**
* Listen to an event from the backend.
*
* @param event Event name. Must include only alphanumeric characters, `-`, `/`, `:` and `_`.
* @param handler Event handler callback.
* @return A promise resolving to a function to unlisten to the event.
*/
async function listen$1(event, windowLabel, handler) {
	return invokeTauriCommand({
		__tauriModule: "Event",
		message: {
			cmd: "listen",
			event,
			windowLabel,
			handler: transformCallback(handler)
		}
	}).then((eventId) => {
		return async () => _unlisten(event, eventId);
	});
}
/**
* Listen to an one-off event from the backend.
*
* @param event Event name. Must include only alphanumeric characters, `-`, `/`, `:` and `_`.
* @param handler Event handler callback.
* @returns A promise resolving to a function to unlisten to the event.
*/
async function once$1(event, windowLabel, handler) {
	return listen$1(event, windowLabel, (eventData) => {
		handler(eventData);
		_unlisten(event, eventData.id).catch(() => {});
	});
}
//#endregion
//#region node_modules/@tauri-apps/api/event.js
/**
* The event system allows you to emit events to the backend and listen to events from it.
*
* This package is also accessible with `window.__TAURI__.event` when [`build.withGlobalTauri`](https://tauri.app/v1/api/config/#buildconfig.withglobaltauri) in `tauri.conf.json` is set to `true`.
* @module
*/
/**
* @since 1.1.0
*/
var TauriEvent;
(function(TauriEvent) {
	TauriEvent["WINDOW_RESIZED"] = "tauri://resize";
	TauriEvent["WINDOW_MOVED"] = "tauri://move";
	TauriEvent["WINDOW_CLOSE_REQUESTED"] = "tauri://close-requested";
	TauriEvent["WINDOW_CREATED"] = "tauri://window-created";
	TauriEvent["WINDOW_DESTROYED"] = "tauri://destroyed";
	TauriEvent["WINDOW_FOCUS"] = "tauri://focus";
	TauriEvent["WINDOW_BLUR"] = "tauri://blur";
	TauriEvent["WINDOW_SCALE_FACTOR_CHANGED"] = "tauri://scale-change";
	TauriEvent["WINDOW_THEME_CHANGED"] = "tauri://theme-changed";
	TauriEvent["WINDOW_FILE_DROP"] = "tauri://file-drop";
	TauriEvent["WINDOW_FILE_DROP_HOVER"] = "tauri://file-drop-hover";
	TauriEvent["WINDOW_FILE_DROP_CANCELLED"] = "tauri://file-drop-cancelled";
	TauriEvent["MENU"] = "tauri://menu";
	TauriEvent["CHECK_UPDATE"] = "tauri://update";
	TauriEvent["UPDATE_AVAILABLE"] = "tauri://update-available";
	TauriEvent["INSTALL_UPDATE"] = "tauri://update-install";
	TauriEvent["STATUS_UPDATE"] = "tauri://update-status";
	TauriEvent["DOWNLOAD_PROGRESS"] = "tauri://update-download-progress";
})(TauriEvent || (TauriEvent = {}));
/**
* Listen to an event. The event can be either global or window-specific.
* See {@link Event.windowLabel} to check the event source.
*
* @example
* ```typescript
* import { listen } from '@tauri-apps/api/event';
* const unlisten = await listen<string>('error', (event) => {
*   console.log(`Got error in window ${event.windowLabel}, payload: ${event.payload}`);
* });
*
* // you need to call unlisten if your handler goes out of scope e.g. the component is unmounted
* unlisten();
* ```
*
* @param event Event name. Must include only alphanumeric characters, `-`, `/`, `:` and `_`.
* @param handler Event handler callback.
* @returns A promise resolving to a function to unlisten to the event.
* Note that removing the listener is required if your listener goes out of scope e.g. the component is unmounted.
*
* @since 1.0.0
*/
async function listen(event, handler) {
	return listen$1(event, null, handler);
}
/**
* Listen to an one-off event. See {@link listen} for more information.
*
* @example
* ```typescript
* import { once } from '@tauri-apps/api/event';
* interface LoadedPayload {
*   loggedIn: boolean,
*   token: string
* }
* const unlisten = await once<LoadedPayload>('loaded', (event) => {
*   console.log(`App is loaded, loggedIn: ${event.payload.loggedIn}, token: ${event.payload.token}`);
* });
*
* // you need to call unlisten if your handler goes out of scope e.g. the component is unmounted
* unlisten();
* ```
*
* @param event Event name. Must include only alphanumeric characters, `-`, `/`, `:` and `_`.
* @returns A promise resolving to a function to unlisten to the event.
* Note that removing the listener is required if your listener goes out of scope e.g. the component is unmounted.
*
* @since 1.0.0
*/
async function once(event, handler) {
	return once$1(event, null, handler);
}
/**
* Emits an event to the backend and all Tauri windows.
* @example
* ```typescript
* import { emit } from '@tauri-apps/api/event';
* await emit('frontend-loaded', { loggedIn: true, token: 'authToken' });
* ```
*
* @param event Event name. Must include only alphanumeric characters, `-`, `/`, `:` and `_`.
*
* @since 1.0.0
*/
async function emit(event, payload) {
	return emit$1(event, void 0, payload);
}
//#endregion
export { TauriEvent, emit, listen, once };

//# sourceMappingURL=@tauri-apps_api_event.js.map