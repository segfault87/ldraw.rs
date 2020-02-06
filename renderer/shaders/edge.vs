#version 100

precision mediump float;

attribute vec3 position;
attribute vec3 color;

uniform mat4 projection;
uniform mat4 modelView;
uniform mat4 viewMatrix;
uniform vec3 colorDefault;
uniform vec3 colorEdge;

varying vec3 vColor;

void main(void) {
    vec4 pos4 = vec4(position, 1.0);
    gl_Position = projection * viewMatrix * modelView * pos4;
    if (color.x < -1.0) {
        vColor = colorEdge;
    } else if (color.x < 0.0) {
        vColor = colorDefault;
    } else {
        vColor = color;
    }
}
