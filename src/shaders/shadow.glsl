// Box shadow — soft drop shadow around a rectangular window.
// Uses Gaussian falloff for natural-looking light diffusion.
precision mediump float;

varying vec2 v_coords;
uniform vec2 size;
uniform float alpha;

// Window rect within this element (x, y, w, h in pixels)
uniform vec4 u_window_rect;
// Shadow extent in pixels (shadow is ~invisible at this distance)
uniform float u_radius;
// Shadow RGBA color (alpha controls peak opacity at the window edge)
uniform vec4 u_color;
// Corner rounding radius
uniform float u_corner_radius;

void main() {
    vec2 pixel = v_coords * size;

    // Rounded rectangle signed distance function
    vec2 half_size = u_window_rect.zw * 0.5;
    vec2 center = u_window_rect.xy + half_size;
    vec2 q = abs(pixel - center) - half_size + vec2(u_corner_radius);
    float dist = length(max(q, 0.0)) - u_corner_radius;

    // Gaussian falloff: sigma = radius/3 so shadow is ~invisible at u_radius
    float sigma = u_radius / 3.0;
    float shadow = exp(-(dist * dist) / (2.0 * sigma * sigma));
    // Only apply shadow outside the window
    shadow *= step(0.0, dist);

    gl_FragColor = u_color * shadow * alpha;
}
