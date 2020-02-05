#version 100

precision highp float;

attribute vec3 position;
attribute vec3 color;

uniform mat4 projection;
uniform mat4 modelView;
uniform mat4 viewMatrix;
uniform vec4 instanceColor;
uniform vec4 instanceColorEdge;

/* not used */
uniform vec4 lightColor;
uniform vec4 lightDirection;

varying vec4 vColor;

void main(void) {
    vec4 pos4 = vec4(position, 1.0);
    gl_Position = projection * viewMatrix * modelView * pos4;
    if (color.x < -1.0) {
        vColor = instanceColorEdge;
    } else if (color.x < 0.0) {
        vColor = instanceColor;
    } else {
        vColor = vec4(color, 1.0);
    }
}